"""PDF text extraction via pypdf."""

from __future__ import annotations

from pathlib import Path
from typing import Any


def extract_pdf(path: str) -> dict[str, Any]:
    from pypdf import PdfReader

    reader = PdfReader(path)
    pages: list[str] = []
    for page in reader.pages:
        try:
            pages.append(page.extract_text() or "")
        except Exception:  # noqa: BLE001
            pages.append("")
    metadata: dict[str, Any] = {}
    if reader.metadata:
        for key in ("title", "author", "subject", "creator", "producer"):
            value = getattr(reader.metadata, key, None)
            if value is not None:
                metadata[key] = str(value)
    return {
        "path": str(path),
        "page_count": len(pages),
        "text": "\n\n".join(pages),
        "pages": pages,
        "metadata": metadata,
    }
