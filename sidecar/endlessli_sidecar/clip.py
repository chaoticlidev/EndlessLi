"""Web clipping: fetch a page over HTTPS and extract readable text."""

from __future__ import annotations

import ipaddress
import re
import socket
from dataclasses import dataclass
from urllib.parse import urljoin, urlparse

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

#: Redirects followed before the clip fails.
_MAX_REDIRECTS = 5

#: Hostnames/suffixes that always name the local machine or a private zone.
_BLOCKED_HOSTS = {"localhost", "localhost.localdomain", "ip6-localhost", "ip6-loopback"}
_BLOCKED_HOST_SUFFIXES = (".local", ".internal", ".localhost", ".home.arpa")


def _is_public_address(address: str) -> bool:
    """True only for a globally routable unicast address.

    This rejects loopback, link-local (169.254/16, fe80::/10), RFC1918,
    shared/CGNAT, multicast, reserved, and unspecified ranges so a search
    result cannot point the clipper at a device on the user's network.
    """
    try:
        ip = ipaddress.ip_address(address)
    except ValueError:
        return False
    return ip.is_global and not (
        ip.is_private
        or ip.is_loopback
        or ip.is_link_local
        or ip.is_multicast
        or ip.is_reserved
        or ip.is_unspecified
    )


def _resolve_addresses(host: str, port: int) -> list[str]:
    """Resolves ``host`` to its addresses. Separated so tests never touch DNS."""
    infos = socket.getaddrinfo(host, port, type=socket.SOCK_STREAM)
    return sorted({info[4][0] for info in infos})


@dataclass
class ClipResult:
    """Readable text plus provenance for citation."""

    url: str
    title: str
    text: str
    truncated: bool


def _check_url(url: str) -> str:
    """Validates a URL and refuses any target on a private/non-public network.

    HTTPS-only keeps most intranet targets out (self-signed certs fail), and
    the host/IP check blocks loopback, RFC1918, link-local, and private DNS
    zones. Callers re-run this on every redirect hop.
    """
    cleaned = url.strip()
    parsed = urlparse(cleaned)
    if parsed.scheme != "https" or not parsed.netloc:
        raise ValueError(f"refusing to fetch non-HTTPS URL: {url!r}")
    host = (parsed.hostname or "").rstrip(".").lower()
    if not host:
        raise ValueError(f"refusing to fetch URL without a host: {url!r}")
    if host in _BLOCKED_HOSTS or any(host.endswith(suffix) for suffix in _BLOCKED_HOST_SUFFIXES):
        raise ValueError(f"refusing to fetch non-public host: {host!r}")
    try:
        literal = ipaddress.ip_address(host)
    except ValueError:
        addresses = _resolve_addresses(host, parsed.port or 443)
        if not addresses or not all(_is_public_address(address) for address in addresses):
            raise ValueError(f"refusing to fetch non-public host: {host!r}") from None
    else:
        if not _is_public_address(str(literal)):
            raise ValueError(f"refusing to fetch non-public address: {host!r}")
    return cleaned


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

    def follow_redirects(session: requests.Session, start: str) -> requests.Response:
        """Fetches ``start`` one hop at a time so every redirect target is
        re-validated (a public URL must not bounce to a private one)."""
        current = start
        for _ in range(_MAX_REDIRECTS + 1):
            response = session.get(
                current,
                headers={"User-Agent": USER_AGENT, "Accept": "text/html"},
                timeout=timeout_secs,
                stream=True,
                allow_redirects=False,
            )
            if response.status_code not in (301, 302, 303, 307, 308):
                return response
            location = response.headers.get("Location")
            response.close()
            if not location:
                raise ValueError("redirect without a Location header")
            current = _check_url(urljoin(current, location))
        raise ValueError(f"too many redirects (limit {_MAX_REDIRECTS})")

    def fetch() -> requests.Response:
        session = requests.Session()
        try:
            response = follow_redirects(session, cleaned_url)
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
