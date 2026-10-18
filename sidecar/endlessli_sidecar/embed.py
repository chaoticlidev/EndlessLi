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
import threading
from typing import Any

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


def embed_texts(texts: list[str], model: str | None = None) -> dict[str, Any]:
    """Embeds ``texts`` in order, returning ``{model, dim, embeddings}``.

    Raises :class:`EmbeddingUnavailable` when the backend is missing and
    :class:`ValueError` for an unsupported model.
    """
    name = resolve_model(model)
    if not _fastembed_available():
        raise EmbeddingUnavailable(
            "embedding backend not installed; run "
            "`uv pip install -r sidecar/requirements-embeddings.txt`"
        )
    if name not in SUPPORTED_MODELS:
        raise ValueError(f"unsupported embedding model: {name}")
    embedder = _load(name)
    prepared = [text[:MAX_TEXT_CHARS] for text in texts]
    vectors = embedder.embed(prepared)
    embeddings = [[float(value) for value in vector] for vector in vectors]
    return {"model": name, "dim": SUPPORTED_MODELS[name], "embeddings": embeddings}
