"""Pluggable web search backends with fallback across providers.

Each :class:`SearchProvider` knows how to talk to one upstream search API and
returns normalized :class:`SearchResult` objects.  The module-level
:func:`search_with_fallback` walks a caller-supplied priority list and stops at
the first backend that returns a non-empty result set.

Provider registry order doubles as the default fallback list when the caller
does not specify one, so the most reliable keyless/free options come first.
"""

from __future__ import annotations

import re
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
    """Per-call configuration carrying every credential the providers need.

    Keys are intentionally flat so the Rust side can forward a JSON object
    directly from user settings without per-field mapping.  Unknown fields are
    ignored by providers that don't use them.
    """

    searxng_url: str = ""
    brave_key: str = ""
    tavily_key: str = ""
    exa_key: str = ""
    keenable_key: str = ""
    firecrawl_key: str = ""
    bing_key: str = ""
    parallel_key: str = ""
    anysearch_key: str = ""
    perplexity_key: str = ""
    serpbase_key: str = ""
    you_key: str = ""
    safe_search: str = "off"
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


def _map_safe_search(ctx: ProviderContext, mapping: dict[str, Any]) -> Any:
    """Resolves the provider-specific safe-search value from the context."""
    value = mapping.get(ctx.safe_search)
    return value if value is not None else mapping.get("off")


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


class DuckDuckGoLiteProvider:
    """DuckDuckGo Lite HTML search (POST to lite.duckduckgo.com)."""

    def name(self) -> str:
        return "ddg_lite"

    def requires_key(self) -> bool:
        return False

    def search(self, query: str, max_results: int, ctx: ProviderContext) -> list[SearchResult]:
        def run() -> list[SearchResult]:
            response = requests.post(
                "https://lite.duckduckgo.com/lite/",
                data={"q": query, "kl": "us-en", "df": ""},
                headers={"User-Agent": USER_AGENT, "Accept": "text/html"},
                timeout=ctx.timeout_secs,
            )
            response.raise_for_status()
            html = response.text
            results: list[SearchResult] = []
            pattern = re.compile(
                r'<a[^>]+class="result-link"[^>]*href="([^"]+)"[^>]*>(.*?)</a>.*?<td[^>]*class="result-snippet">(.*?)</td>',
                re.DOTALL,
            )
            for url, title, snippet in pattern.findall(html):
                cleaned_title = re.sub(r"<[^>]+>", "", title).strip()
                cleaned_snippet = re.sub(r"<[^>]+>", "", snippet).strip()
                if url and cleaned_title:
                    results.append(
                        SearchResult(
                            title=cleaned_title,
                            url=url,
                            snippet=cleaned_snippet,
                            provider=self.name(),
                        )
                    )
                    if len(results) >= max_results:
                        break
            if not results:
                alt_pattern = re.compile(
                    r'<a[^>]+rel="nofollow"[^>]*href="([^"]+)"[^>]*>(.*?)</a>.*?<td[^>]*>(.*?)</td>',
                    re.DOTALL,
                )
                for url, title, snippet in alt_pattern.findall(html):
                    cleaned_title = re.sub(r"<[^>]+>", "", title).strip()
                    cleaned_snippet = re.sub(r"<[^>]+>", "", snippet).strip()
                    if url and cleaned_title:
                        results.append(
                            SearchResult(
                                title=cleaned_title,
                                url=url,
                                snippet=cleaned_snippet,
                                provider=self.name(),
                            )
                        )
                        if len(results) >= max_results:
                            break
            return results

        return with_retry(run)


class DuckDuckGoHtmlProvider:
    """DuckDuckGo HTML search (POST to html.duckduckgo.com)."""

    def name(self) -> str:
        return "ddg_html"

    def requires_key(self) -> bool:
        return False

    def search(self, query: str, max_results: int, ctx: ProviderContext) -> list[SearchResult]:
        def run() -> list[SearchResult]:
            response = requests.post(
                "https://html.duckduckgo.com/html/",
                data={"q": query, "kl": "us-en"},
                headers={"User-Agent": USER_AGENT, "Accept": "text/html"},
                timeout=ctx.timeout_secs,
            )
            response.raise_for_status()
            html = response.text
            results: list[SearchResult] = []
            pattern = re.compile(
                r'<a[^>]+class="result__a"[^>]*href="([^"]+)"[^>]*>(.*?)</a>.*?<a[^>]+class="result__snippet"[^>]*>(.*?)</a>',
                re.DOTALL,
            )
            for url, title, snippet in pattern.findall(html):
                cleaned_title = re.sub(r"<[^>]+>", "", title).strip()
                cleaned_snippet = re.sub(r"<[^>]+>", "", snippet).strip()
                if url and cleaned_title:
                    results.append(
                        SearchResult(
                            title=cleaned_title,
                            url=url,
                            snippet=cleaned_snippet,
                            provider=self.name(),
                        )
                    )
                    if len(results) >= max_results:
                        break
            return results

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

        safe = _map_safe_search(ctx, {"off": 0, "moderate": 1, "strict": 2})

        def run() -> list[SearchResult]:
            response = requests.get(
                "https://api.search.brave.com/res/v1/web/search",
                params={"q": query, "count": min(max_results, 20), "safe": safe},
                headers={"X-Subscription-Token": ctx.brave_key, "Accept": "application/json"},
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


class TavilyProvider:
    """Tavily Search API (requires a caller-supplied API key)."""

    def name(self) -> str:
        return "tavily"

    def requires_key(self) -> bool:
        return True

    def search(self, query: str, max_results: int, ctx: ProviderContext) -> list[SearchResult]:
        if not ctx.tavily_key.strip():
            raise ValueError("tavily_key is required for the tavily provider")

        safe = _map_safe_search(ctx, {"off": "off", "moderate": "moderate", "strict": "strict"})

        def run() -> list[SearchResult]:
            response = requests.post(
                "https://api.tavily.io/2024-07-09/search_with_image",
                json={
                    "api_key": ctx.tavily_key,
                    "query": query,
                    "max_results": min(max_results, 10),
                    "include_raw_content": False,
                    "include_images": False,
                    "safe_search": safe,
                },
                headers={"User-Agent": USER_AGENT},
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
                        snippet=str(hit.get("content", "") or hit.get("snippet", "") or ""),
                        provider=self.name(),
                    )
                )
            return out

        return with_retry(run)


class ExaProvider:
    """Exa Search API (requires a caller-supplied API key)."""

    def name(self) -> str:
        return "exa"

    def requires_key(self) -> bool:
        return True

    def search(self, query: str, max_results: int, ctx: ProviderContext) -> list[SearchResult]:
        if not ctx.exa_key.strip():
            raise ValueError("exa_key is required for the exa provider")

        def run() -> list[SearchResult]:
            response = requests.post(
                "https://api.exa.ai/search",
                json={
                    "query": query,
                    "numResults": min(max_results, 10),
                    "contents": {"text": True},
                    "category": "general",
                    "includeImages": False,
                },
                headers={"X-API-Key": ctx.exa_key, "Content-Type": "application/json"},
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
                        snippet=str(hit.get("snippet", "") or hit.get("summary", "") or ""),
                        provider=self.name(),
                    )
                )
            return out

        return with_retry(run)


class KeenableProvider:
    """Keenable Search API (requires a caller-supplied API key)."""

    def name(self) -> str:
        return "keenable"

    def requires_key(self) -> bool:
        return True

    def search(self, query: str, max_results: int, ctx: ProviderContext) -> list[SearchResult]:
        if not ctx.keenable_key.strip():
            raise ValueError("keenable_key is required for the keenable provider")

        safe = _map_safe_search(ctx, {"off": "off", "moderate": "moderate", "strict": "strict"})

        def run() -> list[SearchResult]:
            response = requests.post(
                "https://api.keenable.com/v1/search",
                json={"query": query, "top_k": min(max_results, 10), "safe_search": safe},
                headers={"Authorization": f"Bearer {ctx.keenable_key}", "Content-Type": "application/json"},
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
                        snippet=str(hit.get("snippet", "") or hit.get("description", "") or ""),
                        provider=self.name(),
                    )
                )
            return out

        return with_retry(run)


class FirecrawlProvider:
    """Firecrawl Search API (requires a caller-supplied API key)."""

    def name(self) -> str:
        return "firecrawl"

    def requires_key(self) -> bool:
        return True

    def search(self, query: str, max_results: int, ctx: ProviderContext) -> list[SearchResult]:
        if not ctx.firecrawl_key.strip():
            raise ValueError("firecrawl_key is required for the firecrawl provider")

        def run() -> list[SearchResult]:
            response = requests.post(
                "https://api.firecrawl.dev/v1/search",
                json={"query": query, "limit": min(max_results, 20), "scrapeOptions": {"formats": ["markdown"]}},
                headers={"Authorization": f"Bearer {ctx.firecrawl_key}", "Content-Type": "application/json"},
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
                        title=str(hit.get("title", "") or hit.get("metadata", {}).get("title", "") or ""),
                        url=str(hit.get("url", "") or ""),
                        snippet=str(hit.get("description", "") or hit.get("markdown", "") or ""),
                        provider=self.name(),
                    )
                )
            return out

        return with_retry(run)


class BingProvider:
    """Bing Web Search API v7 (requires a caller-supplied subscription key)."""

    def name(self) -> str:
        return "bing"

    def requires_key(self) -> bool:
        return True

    def search(self, query: str, max_results: int, ctx: ProviderContext) -> list[SearchResult]:
        if not ctx.bing_key.strip():
            raise ValueError("bing_key is required for the bing provider")

        safe = _map_safe_search(ctx, {"off": 0, "moderate": 1, "strict": 2})

        def run() -> list[SearchResult]:
            response = requests.get(
                "https://api.bing.microsoft.com/v7.0/search",
                params={
                    "q": query,
                    "count": min(max_results, 50),
                    "offset": 0,
                    "safesearch": "Off Moderate Strict"[safe * 6 : safe * 6 + 6].strip(),
                },
                headers={"Ocp-Apim-Subscription-Key": ctx.bing_key},
                timeout=ctx.timeout_secs,
            )
            response.raise_for_status()
            payload: Any = response.json()
            web = payload.get("webPages", {}) if isinstance(payload, dict) else {}
            raw = web.get("value", []) if isinstance(web, dict) else []
            out = []
            for hit in raw[:max_results]:
                if not isinstance(hit, dict) or not hit.get("url"):
                    continue
                out.append(
                    SearchResult(
                        title=str(hit.get("name", "") or ""),
                        url=str(hit.get("url", "") or ""),
                        snippet=str(hit.get("snippet", "") or ""),
                        provider=self.name(),
                    )
                )
            return out

        return with_retry(run)


class ParallelProvider:
    """Parallel Search API (requires a caller-supplied API key)."""

    def name(self) -> str:
        return "parallel"

    def requires_key(self) -> bool:
        return True

    def search(self, query: str, max_results: int, ctx: ProviderContext) -> list[SearchResult]:
        if not ctx.parallel_key.strip():
            raise ValueError("parallel_key is required for the parallel provider")

        safe = _map_safe_search(ctx, {"off": "off", "moderate": "moderate", "strict": "strict"})

        def run() -> list[SearchResult]:
            response = requests.post(
                "https://api.parallelapi.io/search",
                json={"query": query, "num": min(max_results, 10), "safe": safe},
                headers={"x-api-key": ctx.parallel_key, "Content-Type": "application/json"},
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
                        snippet=str(hit.get("snippet", "") or hit.get("description", "") or ""),
                        provider=self.name(),
                    )
                )
            return out

        return with_retry(run)


class AnySearchProvider:
    """AnySearch AI API (requires a caller-supplied API key)."""

    def name(self) -> str:
        return "anysearch"

    def requires_key(self) -> bool:
        return True

    def search(self, query: str, max_results: int, ctx: ProviderContext) -> list[SearchResult]:
        if not ctx.anysearch_key.strip():
            raise ValueError("anysearch_key is required for the anysearch provider")

        def run() -> list[SearchResult]:
            response = requests.post(
                "https://api.anysearch.ai/v1/search",
                json={"query": query, "top_k": min(max_results, 10)},
                headers={"Authorization": f"Bearer {ctx.anysearch_key}", "Content-Type": "application/json"},
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
                        snippet=str(hit.get("snippet", "") or hit.get("description", "") or ""),
                        provider=self.name(),
                    )
                )
            return out

        return with_retry(run)


class PerplexityProvider:
    """Perplexity Search API (requires a caller-supplied API key)."""

    def name(self) -> str:
        return "perplexity"

    def requires_key(self) -> bool:
        return True

    def search(self, query: str, max_results: int, ctx: ProviderContext) -> list[SearchResult]:
        if not ctx.perplexity_key.strip():
            raise ValueError("perplexity_key is required for the perplexity provider")

        def run() -> list[SearchResult]:
            response = requests.post(
                "https://api.perplexity.ai/search",
                json={"query": query, "max_results": min(max_results, 10)},
                headers={"Authorization": f"Bearer {ctx.perplexity_key}", "Content-Type": "application/json"},
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
                        snippet=str(hit.get("snippet", "") or hit.get("content", "") or ""),
                        provider=self.name(),
                    )
                )
            return out

        return with_retry(run)


class SerpBaseProvider:
    """SerpBase DuckDuckGo search via their paid API (requires an API key).

    POSTs to /duckduckgo/search with {"query": "..."} and returns a normalized
    source_data envelope.
    """

    def name(self) -> str:
        return "serpbase"

    def requires_key(self) -> bool:
        return True

    def search(self, query: str, max_results: int, ctx: ProviderContext) -> list[SearchResult]:
        if not ctx.serpbase_key.strip():
            raise ValueError("serpbase_key is required for the serpbase provider")

        def run() -> list[SearchResult]:
            response = requests.post(
                "https://api.serpbase.dev/duckduckgo/search",
                json={"query": query, "limit": min(max_results, 20)},
                headers={"X-API-Key": ctx.serpbase_key, "Content-Type": "application/json"},
                timeout=ctx.timeout_secs,
            )
            response.raise_for_status()
            payload: Any = response.json()
            raw = payload.get("organic", []) if isinstance(payload, dict) else []
            out = []
            for hit in raw[:max_results]:
                if not isinstance(hit, dict) or not hit.get("url"):
                    continue
                out.append(
                    SearchResult(
                        title=str(hit.get("title", "") or ""),
                        url=str(hit.get("url", "") or ""),
                        snippet=str(hit.get("snippet", "") or hit.get("description", "") or ""),
                        provider=self.name(),
                    )
                )
            return out

        return with_retry(run)


class YouComApiProvider:
    """You.com search API (requires a caller-supplied API key)."""

    def name(self) -> str:
        return "you"

    def requires_key(self) -> bool:
        return True

    def search(self, query: str, max_results: int, ctx: ProviderContext) -> list[SearchResult]:
        if not ctx.you_key.strip():
            raise ValueError("you_key is required for the you provider")

        def run() -> list[SearchResult]:
            response = requests.get(
                "https://api.beingapi.com/v1/search",
                params={"query": query, "num": min(max_results, 20)},
                headers={"x-api-key": ctx.you_key, "User-Agent": USER_AGENT},
                timeout=ctx.timeout_secs,
            )
            response.raise_for_status()
            payload: Any = response.json()
            raw = payload.get("results", []) if isinstance(payload, dict) else []
            out = []
            for hit in raw[:max_results]:
                if not isinstance(hit, dict) or not hit.get("link"):
                    continue
                out.append(
                    SearchResult(
                        title=str(hit.get("title", "") or ""),
                        url=str(hit.get("link", "") or ""),
                        snippet=str(hit.get("snippet", "") or ""),
                        provider=self.name(),
                    )
                )
            return out

        return with_retry(run)


#: Registry of known backends in default preference order.
PROVIDERS: dict[str, SearchProvider] = {
    provider.name(): provider
    for provider in (
        DDGSProvider(),
        DuckDuckGoLiteProvider(),
        DuckDuckGoHtmlProvider(),
        SearXNGProvider(),
        BraveProvider(),
        FirecrawlProvider(),
        KeenableProvider(),
        ExaProvider(),
        TavilyProvider(),
        SerpBaseProvider(),
        BingProvider(),
        ParallelProvider(),
        AnySearchProvider(),
        PerplexityProvider(),
        YouComApiProvider(),
    )
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
