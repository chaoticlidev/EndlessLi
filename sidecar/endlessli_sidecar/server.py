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
from .extract import extract_pdf
from .chunk import chunk_documents
from .cluster import kmeans
from .search import PROVIDERS, ProviderContext, search_with_fallback
from . import tts as tts_engine

#: Payload ceilings for the raw JSON-RPC surface (defense in depth; the Rust
#: commands also validate before forwarding).
MAX_EMBED_TEXT_CHARS = 100_000
MAX_EMBED_TOTAL_CHARS = 1_000_000
MAX_CHUNK_DOCS = 1_000
MAX_CHUNK_TOTAL_CHARS = 10_000_000
MAX_URL_CHARS = 8_192
MAX_QUERY_CHARS = 8_000

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
    if len(query) > MAX_QUERY_CHARS:
        raise ParamError(f"query exceeds the {MAX_QUERY_CHARS}-character limit")
    max_results = params.get("max_results", 10)
    if not isinstance(max_results, int) or max_results < 1:
        raise ParamError("max_results must be a positive integer")
    providers = params.get("providers", params.get("provider_names", ["ddgs"]))
    if not isinstance(providers, list) or not providers or not all(isinstance(p, str) for p in providers):
        raise ParamError("providers must be a non-empty string list")
    safe_search = str(params.get("safe_search", "off") or "off")
    if safe_search not in {"off", "moderate", "strict"}:
        safe_search = "off"
    ctx = ProviderContext(
        searxng_url=str(params.get("searxng_url", "") or ""),
        brave_key=str(params.get("brave_key", "") or ""),
        tavily_key=str(params.get("tavily_key", "") or ""),
        exa_key=str(params.get("exa_key", "") or ""),
        keenable_key=str(params.get("keenable_key", "") or ""),
        firecrawl_key=str(params.get("firecrawl_key", "") or ""),
        bing_key=str(params.get("bing_key", "") or ""),
        parallel_key=str(params.get("parallel_key", "") or ""),
        anysearch_key=str(params.get("anysearch_key", "") or ""),
        perplexity_key=str(params.get("perplexity_key", "") or ""),
        serpbase_key=str(params.get("serpbase_key", "") or ""),
        you_key=str(params.get("you_key", "") or ""),
        safe_search=safe_search,
    )
    try:
        outcome = search_with_fallback(query.strip(), min(max_results, 20), providers, ctx)
    except (ValueError, RuntimeError) as exc:
        raise ParamError(str(exc)) from exc
    except Exception as exc:  # noqa: BLE001 — never crash the sidecar process
        import logging
        logging.getLogger("sidecar").exception("search failed: %s", exc)
        sys.stderr.flush()
        return {"provider": "error", "results": []}
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
    if len(url) > MAX_URL_CHARS:
        raise ParamError(f"url exceeds the {MAX_URL_CHARS}-character limit")
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
    """Embeds a batch, returning one vector per usable input in order.

    ``null`` entries are accepted and filtered by the embedder, matching its
    batch cleanup; any other non-string value is rejected so a type mistake
    surfaces as INVALID_PARAMS instead of being embedded as ``repr(value)``.
    The result's ``indices`` array maps each vector back to its input.
    """
    if not isinstance(params, dict):
        raise ParamError("params must be an object")
    _require(params, "texts")
    texts = params["texts"]
    if not isinstance(texts, list) or not texts:
        raise ParamError("texts must be a non-empty list")
    if not all(isinstance(text, str) or text is None for text in texts):
        raise ParamError("texts must contain only strings or null")
    if len(texts) > MAX_TEXTS:
        raise ParamError(f"too many texts (max {MAX_TEXTS})")
    if any(len(text) > MAX_EMBED_TEXT_CHARS for text in texts if isinstance(text, str)):
        raise ParamError(f"a text exceeds the {MAX_EMBED_TEXT_CHARS}-character limit")
    if sum(len(text) for text in texts if isinstance(text, str)) > MAX_EMBED_TOTAL_CHARS:
        raise ParamError(f"texts exceed the {MAX_EMBED_TOTAL_CHARS}-character limit")
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


def method_pdf_extract(params: Any) -> dict[str, Any]:
    if not isinstance(params, dict):
        raise ParamError("params must be an object")
    _require(params, "path")
    path = params["path"]
    if not isinstance(path, str) or not path.strip():
        raise ParamError("path must be a non-empty string")
    try:
        return extract_pdf(path.strip())
    except Exception as exc:  # noqa: BLE001
        raise SidecarError(f"pdf extract failed: {exc}") from exc


def method_chunk_documents(params: Any) -> dict[str, Any]:
    if not isinstance(params, dict):
        raise ParamError("params must be an object")
    _require(params, "documents")
    documents = params["documents"]
    if not isinstance(documents, list):
        raise ParamError("documents must be a list")
    if len(documents) > MAX_CHUNK_DOCS:
        raise ParamError(f"too many documents (max {MAX_CHUNK_DOCS})")
    total_chars = sum(
        len(doc.get("text", ""))
        for doc in documents
        if isinstance(doc, dict) and isinstance(doc.get("text"), str)
    )
    if total_chars > MAX_CHUNK_TOTAL_CHARS:
        raise ParamError(f"documents exceed the {MAX_CHUNK_TOTAL_CHARS}-character limit")
    chunk_size = params.get("chunk_size", 800)
    overlap = params.get("overlap", 100)
    if not isinstance(chunk_size, int) or chunk_size < 1:
        raise ParamError("chunk_size must be a positive integer")
    if not isinstance(overlap, int) or overlap < 0:
        raise ParamError("overlap must be a non-negative integer")
    try:
        chunks = chunk_documents(documents, chunk_size, overlap)
    except ValueError as exc:
        raise ParamError(str(exc)) from exc
    except Exception as exc:  # noqa: BLE001
        raise SidecarError(f"chunking failed: {exc}") from exc
    return {"chunks": chunks}


def method_cluster_kmeans(params: Any) -> dict[str, Any]:
    if not isinstance(params, dict):
        raise ParamError("params must be an object")
    _require(params, "embeddings", "k")
    embeddings = params["embeddings"]
    if not isinstance(embeddings, list) or not embeddings:
        raise ParamError("embeddings must be a non-empty list")
    k = params["k"]
    if not isinstance(k, int) or k < 1:
        raise ParamError("k must be a positive integer")
    seed = params.get("seed", 42)
    if not isinstance(seed, int):
        raise ParamError("seed must be an integer")
    try:
        return kmeans(embeddings, k, seed)
    except ValueError as exc:
        raise ParamError(str(exc)) from exc
    except Exception as exc:  # noqa: BLE001
        raise SidecarError(f"clustering failed: {exc}") from exc


def method_tts_synthesize(params: Any) -> dict[str, Any]:
    """Synthesizes speech with a local or OpenAI-compatible engine."""
    try:
        return tts_engine.synthesize(params)
    except ValueError as exc:
        raise ParamError(str(exc)) from exc


def method_tts_voices(params: Any) -> dict[str, Any]:
    """Lists voices available to a local engine."""
    try:
        return tts_engine.list_voices(params)
    except ValueError as exc:
        raise ParamError(str(exc)) from exc


METHODS: dict[str, Callable[[Any], Any]] = {
    "health.check": method_health_check,
    "search.providers": method_search_providers,
    "search.query": method_search_query,
    "clip.url": method_clip_url,
    "embed.model_info": method_embed_model_info,
    "embed.texts": method_embed_texts,
    "pdf.extract": method_pdf_extract,
    "chunk.documents": method_chunk_documents,
    "cluster.kmeans": method_cluster_kmeans,
    "tts.synthesize": method_tts_synthesize,
    "tts.voices": method_tts_voices,
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

if __name__ == "__main__":
    serve()