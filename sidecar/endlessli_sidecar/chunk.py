"""Document chunking for Semantic Compression."""

from __future__ import annotations

import itertools
from typing import Any
import re
import sys

_UNPAIRED_SURROGATES = re.compile(r"[\ud800-\udfff]")

def _tokens(text: str) -> int:
    return max(1, len(text) // 4)

def chunk_documents(
    documents: list[dict[str, Any]],
    chunk_size: int = 800,
    overlap: int = 100,
) -> list[dict[str, Any]]:
    if chunk_size < 1:
        raise ValueError("chunk_size must be positive")
    if overlap < 0 or overlap >= chunk_size:
        raise ValueError("overlap must be in [0, chunk_size)")
    out: list[dict[str, Any]] = []
    counter = itertools.count()
    for doc in documents:
        doc_ref = str(doc.get("doc_ref") or doc.get("id") or "")
        text = str(doc.get("text") or doc.get("content") or "")
        if not text.strip():
            continue
        text = _UNPAIRED_SURROGATES.sub("", text)
        text = text[:100_000]
        paragraphs = [p for p in text.split("\n\n") if p.strip()]
        if not paragraphs:
            paragraphs = [text]
        current: list[str] = []
        current_tokens = 0
        char_cursor = 0
        # precompute offsets (O(N) scan instead of O(N^2) search)
        para_offsets: list[tuple[int, int]] = []
        cursor = 0
        for p in paragraphs:
            # Find the start of the paragraph after the cursor
            idx = text.find(p, cursor)
            if idx < 0:
                idx = cursor
            para_offsets.append((idx, idx + len(p)))
            # Move cursor past this paragraph for the next search
            cursor = idx + len(p)
        i = 0
        n = len(paragraphs)
        while i < n:
            # build a chunk starting at i
            chunk_paras: list[int] = []
            ct = 0
            j = i
            while j < n:
                pt = _tokens(paragraphs[j])
                if chunk_paras and ct + pt > chunk_size:
                    break
                chunk_paras.append(j)
                ct += pt
                j += 1
                if ct >= chunk_size:
                    break
            if not chunk_paras:
                chunk_paras.append(i)
                j = i + 1
            start_char = para_offsets[chunk_paras[0]][0]
            end_char = para_offsets[chunk_paras[-1]][1]
            chunk_text = "\n\n".join(paragraphs[k] for k in chunk_paras)
            out.append(
                {
                    "id": f"chunk-{next(counter)}",
                    "doc_ref": doc_ref,
                    "chunk_index": len([c for c in out if c["doc_ref"] == doc_ref]),
                    "text": chunk_text,
                    "char_start": start_char,
                    "char_end": end_char,
                }
            )
            # advance with overlap
            if j >= n:
                break
            # find overlap start
            overlap_tokens = 0
            new_i = j
            for k in range(j - 1, i - 1, -1):
                overlap_tokens += _tokens(paragraphs[k])
                if overlap_tokens >= overlap:
                    new_i = k
                    break
            else:
                new_i = i
            if new_i <= i:
                new_i = i + 1
            i = new_i
    return out
