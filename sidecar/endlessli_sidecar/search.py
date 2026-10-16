"""Pluggable web search backends with fallback across providers."""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any, Protocol

import requests

from .retry import with_retry

#: Default result count when the caller does not specify one.
DEFAULT_MAX_RESULTS = 10

#: Browser-like user agent; some endpoints reject the library default.
USER_AGENT = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) EndlessLi/0.4"


@dataclass
class SearchResult:
    """One normalized search hit, whatever the backend."""

    title: str
    url: str
    snippet: str
    provider: str


@dataclass
class ProviderContext:
    """Per-call configuration handed to providers that need secrets."""

    searxng_url: str = ""
    brave_key: str = ""
    timeout_secs: float = 20.0


class SearchProvider(Protocol):
    """A web search backend."""

    def name(self) -> str:
        """Stable provider id used in logs, cache keys, and results."""
        ...

    def requires_key(self) -> bool:
        """Whether this backend needs a caller-supplied credential."""
        ...

    def search(self, query: str, max_results: int, ctx: ProviderContext) -> list[SearchResult]:
        """Returns up to ``max_results`` hits, raising on failure."""
        ...


class DDGSProvider:
    """DuckDuckGo search via the ``ddgs`` scraper.

    Free and keyless, but unofficial: treat failures as routine and let the
    fallback chain cover outages.
    """

    def name(self) -> str:
        return "ddgs"

    def requires_key(self) -> bool:
        return False

    def search(self, query: str, max_results: int, ctx: ProviderContext) -> list[SearchResult]:
        from ddgs import DDGS

        def run() -> list[SearchResult]:
            with DDGS(timeout=int(ctx.timeout_secs)) as ddgs:
                # Positional keywords keep working across ddgs major versions.
                raw = ddgs.text(query, max_results=max_results) or []
            return [
                SearchResult(
                    title=str(hit.get("title", "") or ""),
                    url=str(hit.get("href", "") or ""),
                    snippet=str(hit.get("body", "") or ""),
                    provider=self.name(),
                )
                for hit in raw
                if hit.get("href")
            ]

        return with_retry(run)


class SearXNGProvider:
    """Self-hosted SearXNG instance speaking its JSON API."""

    def name(self) -> str:
        return "searxng"

    def requires_key(self) -> bool:
        return False

    def search(self, query: str, max_results: int, ctx: ProviderContext) -> list[SearchResult]:
        base = ctx.searxng_url.strip().rstrip("/")
        if not base:
            raise ValueError("searxng_url is required for the searxng provider")

        def run() -> list[SearchResult]:
            response = requests.get(
                f"{base}/search",
                params={"q": query, "format": "json", "pageno": 1},
                headers={"User-Agent": USER_AGENT, "Accept": "application/json"},
                timeout=ctx.timeout_secs,
            )
            response.raise_for_status()
            payload: Any = response.json()
            raw = payload.get("results", []) if isinstance(payload, dict) else []
            out = []
            for hit in raw[:max_results]:
                if not isinstance(hit, dict) or not hit.get("url"):
                    continue
                out.append(
                    SearchResult(
                        title=str(hit.get("title", "") or ""),
                        url=str(hit.get("url", "") or ""),
                        snippet=str(hit.get("content", "") or ""),
                        provider=self.name(),
                    )
                )
            return out

        return with_retry(run)


class BraveProvider:
    """Brave Search API (requires a caller-supplied subscription token)."""

    def name(self) -> str:
        return "brave"

    def requires_key(self) -> bool:
        return True

    def search(self, query: str, max_results: int, ctx: ProviderContext) -> list[SearchResult]:
        if not ctx.brave_key.strip():
            raise ValueError("brave_key is required for the brave provider")

        def run() -> list[SearchResult]:
            response = requests.get(
                "https://api.search.brave.com/res/v1/web/search",
                params={"q": query, "count": min(max_results, 20)},
                headers={
                    "X-Subscription-Token": ctx.brave_key,
                    "Accept": "application/json",
                },
                timeout=ctx.timeout_secs,
            )
            response.raise_for_status()
            payload: Any = response.json()
            web = payload.get("web", {}) if isinstance(payload, dict) else {}
            raw = web.get("results", []) if isinstance(web, dict) else []
            out = []
            for hit in raw[:max_results]:
                if not isinstance(hit, dict) or not hit.get("url"):
                    continue
                out.append(
                    SearchResult(
                        title=str(hit.get("title", "") or ""),
                        url=str(hit.get("url", "") or ""),
                        snippet=str(hit.get("description", "") or ""),
                        provider=self.name(),
                    )
                )
            return out

        return with_retry(run)


#: Registry of known backends in default preference order.
PROVIDERS: dict[str, SearchProvider] = {
    provider.name(): provider
    for provider in (DDGSProvider(), SearXNGProvider(), BraveProvider())
}


@dataclass
class SearchOutcome:
    """Results plus which backend served them (for cache source labels)."""

    results: list[SearchResult] = field(default_factory=list)
    provider: str = ""


def search_with_fallback(
    query: str,
    max_results: int = DEFAULT_MAX_RESULTS,
    provider_names: list[str] | None = None,
    ctx: ProviderContext | None = None,
) -> SearchOutcome:
    """Runs providers in order until one returns results.

    An empty result set counts as failure and moves to the next provider;
    the last error propagates when every backend fails. Unknown provider
    names fail fast so configuration mistakes surface immediately.
    """
    names = provider_names or ["ddgs"]
    context = ctx or ProviderContext()
    last_error: Exception | None = None
    for name in names:
        provider = PROVIDERS.get(name)
        if provider is None:
            raise ValueError(f"unknown search provider: {name}")
        try:
            results = provider.search(query, max_results, context)
        except Exception as exc:  # noqa: BLE001 - fallback needs any failure
            last_error = exc
            continue
        if results:
            return SearchOutcome(results=results, provider=provider.name())
        last_error = RuntimeError(f"provider {name} returned no results")
    assert last_error is not None  # names is non-empty by construction
    raise last_error
