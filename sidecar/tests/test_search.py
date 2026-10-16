"""Search providers with mocked transports: normalization and fallback."""

import pytest
import requests

from endlessli_sidecar import search
from endlessli_sidecar.search import (
    BraveProvider,
    DDGSProvider,
    ProviderContext,
    SearXNGProvider,
    search_with_fallback,
)


class FakeDDGS:
    """Stand-in for duckduckgo_search.DDGS as a context manager."""

    def __init__(self, *args, **kwargs):
        pass

    def __enter__(self):
        return self

    def __exit__(self, *args):
        return False

    def text(self, keywords, max_results=10):
        assert keywords == "semantic compression"
        return [
            {"title": "A", "href": "https://a.test", "body": "about A"},
            {"title": "B", "href": "", "body": "no url skipped"},
            {"title": "C", "href": "https://c.test", "body": "about C"},
        ]


def test_ddgs_normalizes(monkeypatch):
    monkeypatch.setattr("ddgs.DDGS", FakeDDGS)
    results = DDGSProvider().search("semantic compression", 10, ProviderContext())
    assert [(r.title, r.url, r.provider) for r in results] == [
        ("A", "https://a.test", "ddgs"),
        ("C", "https://c.test", "ddgs"),
    ]


class FakeResponse:
    def __init__(self, payload, status=200):
        self._payload = payload
        self.status_code = status

    def raise_for_status(self):
        if self.status_code >= 400:
            raise requests.HTTPError(f"status {self.status_code}", response=self)

    def json(self):
        return self._payload


def test_searxng_needs_url():
    with pytest.raises(ValueError):
        SearXNGProvider().search("q", 5, ProviderContext())


def test_searxng_normalizes(monkeypatch):
    def fake_get(url, params=None, headers=None, timeout=None):
        assert url == "https://search.test/search"
        assert params["q"] == "styled"
        return FakeResponse({"results": [{"title": "T", "url": "https://t.test", "content": "C"}]})

    monkeypatch.setattr(search.requests, "get", fake_get)
    results = SearXNGProvider().search("styled", 5, ProviderContext(searxng_url="https://search.test/"))
    assert len(results) == 1
    assert results[0].url == "https://t.test"
    assert results[0].provider == "searxng"


def test_brave_needs_key():
    with pytest.raises(ValueError):
        BraveProvider().search("q", 5, ProviderContext())


def test_fallback_uses_second_provider(monkeypatch):
    def boom(*args, **kwargs):
        raise requests.ConnectionError("ddg is down")

    monkeypatch.setattr(DDGSProvider, "search", boom)
    monkeypatch.setattr(
        SearXNGProvider,
        "search",
        lambda self, q, n, ctx: [search.SearchResult("T", "https://t.test", "S", "searxng")],
    )
    outcome = search_with_fallback(
        "q", 5, ["ddgs", "searxng"], ProviderContext(searxng_url="https://s.test")
    )
    assert outcome.provider == "searxng"
    assert outcome.results[0].url == "https://t.test"


def test_unknown_provider_fails_fast():
    with pytest.raises(ValueError):
        search_with_fallback("q", 5, ["yandex"])


def test_all_failing_propagates_last_error(monkeypatch):
    monkeypatch.setattr(DDGSProvider, "search", lambda *a, **k: (_ for _ in ()).throw(RuntimeError("down")))
    with pytest.raises(RuntimeError):
        search_with_fallback("q", 5, ["ddgs"])
