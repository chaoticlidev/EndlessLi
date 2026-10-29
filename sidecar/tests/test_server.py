"""Server dispatch: protocol shape, validation, and error mapping."""

import io
import json

from endlessli_sidecar import server
from endlessli_sidecar.search import SearchOutcome
from endlessli_sidecar.server import dispatch, serve


def test_health_check():
    response = dispatch({"jsonrpc": "2.0", "id": 1, "method": "health.check", "params": {}})
    assert response["id"] == 1
    assert response["result"]["ok"] is True
    assert response["result"]["version"]


def test_search_providers_lists_backends():
    response = dispatch({"jsonrpc": "2.0", "id": 2, "method": "search.providers", "params": {}})
    names = {entry["name"] for entry in response["result"]["providers"]}
    assert names == {
        "ddgs", "ddg_lite", "ddg_html", "searxng", "brave",
        "firecrawl", "keenable", "exa", "tavily",
        "serpbase", "bing", "parallel", "anysearch",
        "perplexity", "you",
    }
    # ddgs, ddg_lite, ddg_html, searxng are keyless
    keyless = {
        entry["name"]
        for entry in response["result"]["providers"]
        if not entry["requires_key"]
    }
    assert keyless == {"ddgs", "ddg_lite", "ddg_html", "searxng"}
    # all paid APIs require a key
    keyed = {
        entry["name"]
        for entry in response["result"]["providers"]
        if entry["requires_key"]
    }
    assert keyed == {
        "brave", "firecrawl", "keenable", "exa", "tavily",
        "serpbase", "bing", "parallel", "anysearch",
        "perplexity", "you",
    }


def test_unknown_method():
    response = dispatch({"jsonrpc": "2.0", "id": 3, "method": "nope", "params": {}})
    assert response["error"]["code"] == server.METHOD_NOT_FOUND
    assert response["id"] == 3


def test_invalid_params_mapped():
    response = dispatch({"jsonrpc": "2.0", "id": 4, "method": "clip.url", "params": {"url": "http://x.test"}})
    assert response["error"]["code"] == server.INVALID_PARAMS


def test_non_object_request_rejected():
    response = dispatch([1, 2, 3])
    assert response["error"]["code"] == server.INVALID_REQUEST


def test_serve_handles_bad_json_and_eof():
    stdin = io.StringIO('not json\n{"jsonrpc": "2.0", "id": 9, "method": "health.check", "params": {}}\n')
    stdout = io.StringIO()
    assert serve(stdin, stdout) == 0
    lines = [json.loads(line) for line in stdout.getvalue().splitlines()]
    assert lines[0]["error"]["code"] == server.PARSE_ERROR
    assert lines[1]["result"]["ok"] is True


def test_search_query_forwards_credentials(monkeypatch):
    """Every credential in the payload reaches the Python ProviderContext."""
    captured = {}

    def fake_search(query, max_results, providers, ctx):
        captured["providers"] = providers
        captured["ctx"] = ctx
        return SearchOutcome(
            provider="brave", results=[]
        )

    monkeypatch.setattr(server, "search_with_fallback", fake_search)
    response = dispatch(
        {
            "jsonrpc": "2.0",
            "id": 10,
            "method": "search.query",
            "params": {
                "query": "fallback",
                # provider_names is the alias older shells send.
                "provider_names": ["brave", "searxng"],
                "searxng_url": "https://search.test",
                "brave_key": "brave-secret",
                "tavily_key": "tavily-secret",
                "you_key": "you-secret",
                "safe_search": "strict",
                "max_results": 3,
            },
        }
    )
    assert response["result"]["provider"] == "brave"
    assert captured["providers"] == ["brave", "searxng"]
    ctx = captured["ctx"]
    assert ctx.searxng_url == "https://search.test"
    assert ctx.brave_key == "brave-secret"
    assert ctx.tavily_key == "tavily-secret"
    assert ctx.you_key == "you-secret"
    assert ctx.safe_search == "strict"


def test_tts_methods_registered():
    assert "tts.synthesize" in server.METHODS
    assert "tts.voices" in server.METHODS


def test_tts_rejects_unknown_engine():
    response = dispatch(
        {
            "jsonrpc": "2.0",
            "id": 11,
            "method": "tts.synthesize",
            "params": {"engine": "espeak", "text": "hello"},
        }
    )
    assert response["error"]["code"] == server.INVALID_PARAMS


def test_tts_rejects_blank_text():
    response = dispatch(
        {
            "jsonrpc": "2.0",
            "id": 12,
            "method": "tts.synthesize",
            "params": {"engine": "piper", "text": "   "},
        }
    )
    assert response["error"]["code"] == server.INVALID_PARAMS

