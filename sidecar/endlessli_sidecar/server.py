"""JSON-RPC 2.0 server over stdio (line-delimited, no batches).

Reads one request object per line from stdin, dispatches to the registered
method table, and writes one response object per line to stdout. Diagnostics
go to stderr; stdout carries protocol bytes only.
"""

from __future__ import annotations

import json
import sys
import traceback
from dataclasses import asdict
from typing import Any, Callable

from . import __version__
from .clip import clip_url
from .embed import MAX_TEXTS, EmbeddingUnavailable, embed_texts, model_info
from .search import PROVIDERS, ProviderContext, search_with_fallback

#: JSON-RPC error codes used by this server.
PARSE_ERROR = -32700
INVALID_REQUEST = -32600
METHOD_NOT_FOUND = -32601
INVALID_PARAMS = -32602
INTERNAL_ERROR = -32603


def _error(code: int, message: str, data: Any = None) -> dict[str, Any]:
    error: dict[str, Any] = {"code": code, "message": message}
    if data is not None:
        error["data"] = data
    return error


def _require(params: Any, *names: str) -> None:
    missing = [name for name in names if not isinstance(params, dict) or name not in params]
    if missing:
        raise ParamError(f"missing params: {', '.join(missing)}")


class ParamError(ValueError):
    """Raised for invalid or missing method parameters."""


def method_health_check(params: Any) -> dict[str, Any]:
    """Liveness probe with the sidecar version."""
    _ = params
    return {"ok": True, "version": __version__}


def method_search_providers(params: Any) -> dict[str, Any]:
    """Lists known search backends and their key requirements."""
    _ = params
    return {
        "providers": [
            {"name": name, "requires_key": provider.requires_key()}
            for name, provider in PROVIDERS.items()
        ]
    }


def method_search_query(params: Any) -> dict[str, Any]:
    """Runs a web search with fallback across the requested providers."""
    if not isinstance(params, dict):
        raise ParamError("params must be an object")
    _require(params, "query")
    query = params["query"]
    if not isinstance(query, str) or not query.strip():
        raise ParamError("query must be a non-empty string")
    max_results = params.get("max_results", 10)
    if not isinstance(max_results, int) or max_results < 1:
        raise ParamError("max_results must be a positive integer")
    providers = params.get("providers", ["ddgs"])
    if not isinstance(providers, list) or not providers or not all(isinstance(p, str) for p in providers):
        raise ParamError("providers must be a non-empty string list")
    ctx = ProviderContext(
        searxng_url=str(params.get("searxng_url", "") or ""),
        brave_key=str(params.get("brave_key", "") or ""),
    )
    try:
        outcome = search_with_fallback(query.strip(), min(max_results, 20), providers, ctx)
    except (ValueError, RuntimeError) as exc:
        raise ParamError(str(exc)) from exc
    except Exception as exc:  # noqa: BLE001 - surfaced as JSON-RPC errors
        raise SidecarError(f"search failed: {exc}") from exc
    return {
        "provider": outcome.provider,
        "results": [asdict(result) for result in outcome.results],
    }


def method_clip_url(params: Any) -> dict[str, Any]:
    """Clips readable text from one HTTPS page."""
    if not isinstance(params, dict):
        raise ParamError("params must be an object")
    _require(params, "url")
    url = params["url"]
    if not isinstance(url, str) or not url.strip():
        raise ParamError("url must be a non-empty string")
    max_chars = params.get("max_chars", 12_000)
    if not isinstance(max_chars, int) or max_chars < 1:
        raise ParamError("max_chars must be a positive integer")
    try:
        result = clip_url(url.strip(), max_chars)
    except ValueError as exc:
        raise ParamError(str(exc)) from exc
    except Exception as exc:  # noqa: BLE001 - surfaced as JSON-RPC errors
        raise SidecarError(f"clip failed: {exc}") from exc
    return asdict(result)


class SidecarError(RuntimeError):
    """A failure worth reporting without a stack trace in the message."""


def method_embed_model_info(params: Any) -> dict[str, Any]:
    """Reports an embedding model's dimension and availability."""
    if params is not None and not isinstance(params, dict):
        raise ParamError("params must be an object")
    model = (params or {}).get("model")
    if model is not None and not isinstance(model, str):
        raise ParamError("model must be a string")
    return model_info(model)


def method_embed_texts(params: Any) -> dict[str, Any]:
    """Embeds a batch of texts, returning one vector per input in order."""
    if not isinstance(params, dict):
        raise ParamError("params must be an object")
    _require(params, "texts")
    texts = params["texts"]
    if not isinstance(texts, list) or not texts or not all(isinstance(text, str) for text in texts):
        raise ParamError("texts must be a non-empty string list")
    if len(texts) > MAX_TEXTS:
        raise ParamError(f"too many texts (max {MAX_TEXTS})")
    model = params.get("model")
    if model is not None and not isinstance(model, str):
        raise ParamError("model must be a string")
    try:
        return embed_texts(texts, model)
    except ValueError as exc:
        raise ParamError(str(exc)) from exc
    except EmbeddingUnavailable as exc:
        raise SidecarError(str(exc)) from exc
    except Exception as exc:  # noqa: BLE001 - surfaced as JSON-RPC errors
        raise SidecarError(f"embedding failed: {exc}") from exc


METHODS: dict[str, Callable[[Any], Any]] = {
    "health.check": method_health_check,
    "search.providers": method_search_providers,
    "search.query": method_search_query,
    "clip.url": method_clip_url,
    "embed.model_info": method_embed_model_info,
    "embed.texts": method_embed_texts,
}


def dispatch(request: Any) -> dict[str, Any]:
    """Routes one decoded request to its method, always returning a response."""
    response: dict[str, Any] = {"jsonrpc": "2.0", "id": None}
    if not isinstance(request, dict):
        response["error"] = _error(INVALID_REQUEST, "request must be a JSON object")
        return response
    response["id"] = request.get("id")
    method = request.get("method")
    handler = METHODS.get(method) if isinstance(method, str) else None
    if handler is None:
        response["error"] = _error(METHOD_NOT_FOUND, f"unknown method: {method!r}")
        return response
    try:
        response["result"] = handler(request.get("params", {}))
    except ParamError as exc:
        response["error"] = _error(INVALID_PARAMS, str(exc))
    except SidecarError as exc:
        response["error"] = _error(INTERNAL_ERROR, str(exc), {"kind": "sidecar"})
    except Exception as exc:  # noqa: BLE001 - protocol must never crash
        traceback.print_exc(file=sys.stderr)
        response["error"] = _error(INTERNAL_ERROR, f"{type(exc).__name__}: {exc}", {"kind": "internal"})
    return response


def serve(stdin: Any = sys.stdin, stdout: Any = sys.stdout) -> int:
    """Serves requests until stdin closes. Returns the process exit code."""
    for line in stdin:
        line = line.strip()
        if not line:
            continue
        try:
            request = json.loads(line)
        except json.JSONDecodeError as exc:
            stdout.write(json.dumps({"jsonrpc": "2.0", "id": None, "error": _error(PARSE_ERROR, f"invalid JSON: {exc}")}) + "\n")
            stdout.flush()
            continue
        stdout.write(json.dumps(dispatch(request)) + "\n")
        stdout.flush()
    return 0
