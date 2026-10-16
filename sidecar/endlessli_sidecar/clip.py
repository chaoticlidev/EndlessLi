"""Web clipping: fetch a page over HTTPS and extract readable text."""

from __future__ import annotations

import re
from dataclasses import dataclass
from urllib.parse import urlparse

import requests
from bs4 import BeautifulSoup

from .retry import with_retry
from .search import USER_AGENT

#: Hard cap on fetched bodies so one page cannot exhaust memory.
MAX_BODY_BYTES = 5 * 1024 * 1024

#: Default text budget per clip; callers may lower it, never exceed the cap.
DEFAULT_MAX_CHARS = 12_000

#: Absolute ceiling shared by every caller.
MAX_CHARS_LIMIT = 50_000

#: Tags that never carry readable content.
_BOILERPLATE_TAGS = {"script", "style", "nav", "header", "footer", "noscript", "form", "aside"}


@dataclass
class ClipResult:
    """Readable text plus provenance for citation."""

    url: str
    title: str
    text: str
    truncated: bool


def _check_url(url: str) -> str:
    """Validates the URL. Only HTTPS is fetched, and only http schemes parse."""
    parsed = urlparse(url.strip())
    if parsed.scheme != "https" or not parsed.netloc:
        raise ValueError(f"refusing to fetch non-HTTPS URL: {url!r}")
    return url.strip()


def _extract_title(soup: BeautifulSoup) -> str:
    for selector in ("meta[property='og:title']", "title"):
        tag = soup.select_one(selector)
        if tag is None:
            continue
        text = tag.get("content", "") if tag.name == "meta" else tag.get_text()
        cleaned = re.sub(r"\s+", " ", str(text)).strip()
        if cleaned:
            return cleaned
    return ""


def _extract_text(soup: BeautifulSoup) -> str:
    for tag in soup(list(_BOILERPLATE_TAGS)):
        tag.decompose()
    for selector in ("article", "main", "[role='main']"):
        main = soup.select_one(selector)
        if main is not None and main.get_text(strip=True):
            return main.get_text(separator="\n")
    body = soup.body
    if body is not None:
        return body.get_text(separator="\n")
    return soup.get_text(separator="\n")


def _clean_text(raw: str) -> str:
    lines = [re.sub(r"[ \t]+", " ", line).strip() for line in raw.splitlines()]
    lines = [line for line in lines if line]
    return "\n".join(lines)


def clip_url(url: str, max_chars: int = DEFAULT_MAX_CHARS, timeout_secs: float = 15.0) -> ClipResult:
    """Fetches ``url`` and returns its readable text.

    Raises on non-HTTPS URLs, network failures (after retry), non-HTML
    content, and pages with no extractable text.
    """
    cleaned_url = _check_url(url)
    chars = min(max(1, max_chars), MAX_CHARS_LIMIT)

    def fetch() -> requests.Response:
        session = requests.Session()
        try:
            response = session.get(
                cleaned_url,
                headers={"User-Agent": USER_AGENT, "Accept": "text/html"},
                timeout=timeout_secs,
                stream=True,
            )
            try:
                response.raise_for_status()
                content_type = response.headers.get("Content-Type", "")
                if content_type and "html" not in content_type.lower():
                    raise ValueError(f"refusing non-HTML content: {content_type}")
                body = b""
                for chunk in response.iter_content(chunk_size=64 * 1024):
                    body += chunk
                    if len(body) > MAX_BODY_BYTES:
                        raise ValueError("page exceeds the 5 MB fetch cap")
                response._content = body  # noqa: SLF001 - avoid a second download
                return response
            finally:
                # Release the connection even though the body was consumed
                # manually; otherwise sockets warn about never closing.
                response.close()
        finally:
            session.close()

    response = with_retry(fetch)
    soup = BeautifulSoup(response.text, "html.parser")
    text = _clean_text(_extract_text(soup))
    if not text:
        raise ValueError("no readable text found on page")
    truncated = len(text) > chars
    return ClipResult(
        url=cleaned_url,
        title=_extract_title(soup),
        text=text[:chars],
        truncated=truncated,
    )
