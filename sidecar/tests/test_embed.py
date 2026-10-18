"""Embedding RPC: model metadata, batch embedding, and graceful degradation.

The real ONNX backend is never loaded here; ``embed`` module internals are
monkeypatched so the tests are fast and network-free.
"""

from endlessli_sidecar import embed, server
from endlessli_sidecar.server import dispatch


def test_model_info_reports_default_when_backend_missing(monkeypatch):
    monkeypatch.setattr(embed, "_fastembed_available", lambda: False)
    response = dispatch({"jsonrpc": "2.0", "id": 1, "method": "embed.model_info", "params": {}})
    result = response["result"]
    assert result["model"] == embed.DEFAULT_MODEL
    assert result["dim"] == 384
    assert result["available"] is False
    assert result["is_default"] is True


def test_model_info_available_when_backend_present(monkeypatch):
    monkeypatch.setattr(embed, "_fastembed_available", lambda: True)
    response = dispatch(
        {"jsonrpc": "2.0", "id": 2, "method": "embed.model_info", "params": {"model": "BAAI/bge-base-en-v1.5"}}
    )
    result = response["result"]
    assert result["model"] == "BAAI/bge-base-en-v1.5"
    assert result["dim"] == 768
    assert result["available"] is True
    assert result["is_default"] is False


def test_embed_texts_returns_one_vector_per_input(monkeypatch):
    class StubEmbedder:
        def embed(self, texts):
            return [[float(len(text)), 0.0, 1.0] for text in texts]

    monkeypatch.setattr(embed, "_fastembed_available", lambda: True)
    monkeypatch.setattr(embed, "_load", lambda model: StubEmbedder())

    response = dispatch(
        {
            "jsonrpc": "2.0",
            "id": 3,
            "method": "embed.texts",
            "params": {"texts": ["ab", "abcd"], "model": embed.DEFAULT_MODEL},
        }
    )
    result = response["result"]
    assert result["model"] == embed.DEFAULT_MODEL
    assert result["dim"] == 384
    assert result["embeddings"] == [[2.0, 0.0, 1.0], [4.0, 0.0, 1.0]]


def test_embed_texts_rejects_empty_list():
    response = dispatch({"jsonrpc": "2.0", "id": 4, "method": "embed.texts", "params": {"texts": []}})
    assert response["error"]["code"] == server.INVALID_PARAMS


def test_embed_texts_rejects_unknown_model(monkeypatch):
    monkeypatch.setattr(embed, "_fastembed_available", lambda: True)
    response = dispatch(
        {"jsonrpc": "2.0", "id": 5, "method": "embed.texts", "params": {"texts": ["x"], "model": "nope/nope"}}
    )
    assert response["error"]["code"] == server.INVALID_PARAMS


def test_embed_texts_maps_missing_backend_to_sidecar_error(monkeypatch):
    monkeypatch.setattr(embed, "_fastembed_available", lambda: False)
    response = dispatch(
        {"jsonrpc": "2.0", "id": 6, "method": "embed.texts", "params": {"texts": ["x"]}}
    )
    assert response["error"]["code"] == server.INTERNAL_ERROR
    assert response["error"]["data"]["kind"] == "sidecar"


def test_embed_texts_truncates_long_input(monkeypatch):
    seen = {}

    class StubEmbedder:
        def embed(self, texts):
            seen["texts"] = texts
            return [[0.0] for _ in texts]

    monkeypatch.setattr(embed, "_fastembed_available", lambda: True)
    monkeypatch.setattr(embed, "_load", lambda model: StubEmbedder())

    long_text = "a" * (embed.MAX_TEXT_CHARS + 50)
    dispatch({"jsonrpc": "2.0", "id": 7, "method": "embed.texts", "params": {"texts": [long_text]}})
    assert len(seen["texts"][0]) == embed.MAX_TEXT_CHARS
