"""Local text embeddings via ONNX Runtime (``fastembed``).

``fastembed`` is an optional dependency: importing this module never requires
it. When it is missing, :func:`model_info` reports ``available: false`` and
:func:`embed_texts` raises :class:`EmbeddingUnavailable`, so search and
clipping keep working on a light install while auto-linking degrades with a
clear diagnostic.

Models are loaded lazily and cached per process. The first use of a model
downloads its ONNX weights from the Hugging Face Hub; ``ENDLESSLI_EMBED_CACHE_DIR``
redirects that cache (the app points it at its data directory).
"""

from __future__ import annotations

import importlib.util
import os
import re
import threading
from typing import Any, Sequence

_UNPAIRED_SURROGATES = re.compile(r"[\ud800-\udfff]")

#: C0/C1 control characters except tab, newline, and carriage return. The
#: tokenizer either rejects these outright or emits useless ``[UNK]`` tokens.
_CONTROL_CHARS = re.compile(r"[\x00-\x08\x0b\x0c\x0e-\x1f\x7f]")

#: Default model: small, fast, 384-dim, meaningful English similarity.
DEFAULT_MODEL = "sentence-transformers/all-MiniLM-L6-v2"

#: Known models and their output dimensions, so ``model_info`` can answer
#: without loading any weights.
SUPPORTED_MODELS: dict[str, int] = {
    DEFAULT_MODEL: 384,
    "BAAI/bge-small-en-v1.5": 384,
    "BAAI/bge-base-en-v1.5": 768,
    "BAAI/bge-large-en-v1.5": 1024,
    "snowflake/snowflake-arctic-embed-xs": 384,
    "snowflake/snowflake-arctic-embed-s": 384,
    "snowflake/snowflake-arctic-embed-m": 768,
}

#: Hard caps so a single RPC stays bounded.
MAX_TEXTS = 256
MAX_TEXT_CHARS = 8000

_lock = threading.Lock()
_models: dict[str, Any] = {}


class EmbeddingUnavailable(RuntimeError):
    """The embedding backend or the requested model weights are missing."""


def _fastembed_available() -> bool:
    return importlib.util.find_spec("fastembed") is not None


def _cache_dir() -> str | None:
    return os.environ.get("ENDLESSLI_EMBED_CACHE_DIR") or None


def resolve_model(model: str | None) -> str:
    """Normalizes a requested model name, defaulting when blank."""
    name = (model or "").strip()
    return name or DEFAULT_MODEL


def clean_text(value: Any) -> str | None:
    """Normalizes one embedding input, returning ``None`` when nothing is left.

    The tokenizer behind ``fastembed`` raises ``TextEncodeInputError`` when it
    receives ``None`` or an empty string, which fails the whole batch. Blank
    input has no meaning to embed anyway, so it is treated as absent. Values
    that are not strings are also rejected rather than stringified: embedding
    ``repr(dict)`` would produce a vector for text the caller never intended.
    """
    if not isinstance(value, str):
        return None
    text = _UNPAIRED_SURROGATES.sub("", value)
    text = _CONTROL_CHARS.sub("", text)
    text = text.strip()
    if not text:
        return None
    return text[:MAX_TEXT_CHARS]


def prepare_texts(texts: Sequence[Any]) -> tuple[list[str], list[int]]:
    """Cleans a batch, returning the usable texts and their input positions.

    ``indices`` is what keeps a filtered batch honest: callers can pair each
    returned vector with the input it came from instead of assuming the
    response has the same length as the request.
    """
    prepared: list[str] = []
    indices: list[int] = []
    for position, value in enumerate(texts):
        cleaned = clean_text(value)
        if cleaned is None:
            continue
        prepared.append(cleaned)
        indices.append(position)
    return prepared, indices


def _load(model: str) -> Any:
    from fastembed import TextEmbedding

    with _lock:
        embedder = _models.get(model)
        if embedder is None:
            kwargs: dict[str, Any] = {"model_name": model}
            cache = _cache_dir()
            if cache:
                kwargs["cache_dir"] = cache
            embedder = TextEmbedding(**kwargs)
            _models[model] = embedder
        return embedder


def model_info(model: str | None = None) -> dict[str, Any]:
    """Returns ``{model, dim, available, is_default}`` for a model."""
    name = resolve_model(model)
    dim = SUPPORTED_MODELS.get(name, 0)
    return {
        "model": name,
        "dim": dim,
        "available": _fastembed_available() and dim > 0,
        "is_default": name == DEFAULT_MODEL,
    }


def embed_texts(texts: Sequence[Any], model: str | None = None) -> dict[str, Any]:
    """Embeds the usable entries of ``texts``, preserving input order.

    Blank strings, ``None``, and non-string values are filtered out before the
    tokenizer sees them. ``indices`` maps every returned vector back to its
    position in ``texts``, so a filtered batch stays alignable instead of
    silently shifting vectors onto the wrong documents.
    """
    name = resolve_model(model)
    if not _fastembed_available():
        raise EmbeddingUnavailable(
            "embedding backend not installed; run "
            "`uv pip install -r sidecar/requirements-embeddings.txt`"
        )
    if name not in SUPPORTED_MODELS:
        raise ValueError(f"unsupported embedding model: {name}")

    prepared, indices = prepare_texts(texts)
    if not prepared:
        return {
            "model": name,
            "dim": SUPPORTED_MODELS[name],
            "embeddings": [],
            "indices": [],
        }

    embedder = _load(name)
    vectors = embedder.embed(prepared)
    embeddings = [[float(value) for value in vector] for vector in vectors]
    if len(embeddings) != len(prepared):
        raise RuntimeError(
            f"embedder returned {len(embeddings)} vectors for {len(prepared)} texts"
        )
    return {
        "model": name,
        "dim": SUPPORTED_MODELS[name],
        "embeddings": embeddings,
        "indices": indices,
    }
