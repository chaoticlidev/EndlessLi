"""Clipper against fixture HTML with a mocked transport: no network."""

from pathlib import Path

import pytest

from endlessli_sidecar import clip as clip_module
from endlessli_sidecar.clip import clip_url

FIXTURE = Path(__file__).parent / "fixtures" / "article.html"


class FakeResponse:
    def __init__(self, text):
        self.text = text
        self.headers = {"Content-Type": "text/html; charset=utf-8"}
        self.status_code = 200

    def raise_for_status(self):
        pass

    def close(self):
        pass

    def iter_content(self, chunk_size=65536):
        data = self.text.encode("utf-8")
        yield data


class FakeSession:
    def __init__(self, text):
        self._text = text
        self.seen_urls = []

    def __enter__(self):
        return self

    def __exit__(self, *args):
        return False

    def close(self):
        pass

    def get(self, url, headers=None, timeout=None, stream=False):
        self.seen_urls.append(url)
        return FakeResponse(self._text)


def test_extracts_article_text_and_title(monkeypatch):
    html = FIXTURE.read_text(encoding="utf-8")
    monkeypatch.setattr(clip_module.requests, "Session", lambda: FakeSession(html))
    result = clip_url("https://example.test/article")
    assert result.url == "https://example.test/article"
    assert result.title == "Semantic Compression Explained"
    assert "Semantic compression selects representative chunks" in result.text
    assert "Clustering groups similar chunks" in result.text
    assert not result.truncated
    # Boilerplate and code never leak into readable text.
    assert "Site Header" not in result.text
    assert "Copyright" not in result.text
    assert "tracker" not in result.text


def test_truncates_long_pages(monkeypatch):
    html = FIXTURE.read_text(encoding="utf-8")
    monkeypatch.setattr(clip_module.requests, "Session", lambda: FakeSession(html))
    result = clip_url("https://example.test/article", max_chars=20)
    assert result.truncated
    assert len(result.text) == 20


def test_refuses_plain_http():
    with pytest.raises(ValueError):
        clip_url("http://example.test/article")


@pytest.mark.parametrize("url", ["not a url", "ftp://files.test/x", ""])
def test_refuses_non_urls(url):
    with pytest.raises(ValueError):
        clip_url(url)


def test_refuses_non_html(monkeypatch):
    class PdfSession(FakeSession):
        def get(self, *args, **kwargs):
            response = FakeResponse("")
            response.headers = {"Content-Type": "application/pdf"}
            return response

    monkeypatch.setattr(clip_module.requests, "Session", lambda: PdfSession(""))
    with pytest.raises(ValueError):
        clip_url("https://example.test/paper.pdf")
