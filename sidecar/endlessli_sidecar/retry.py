"""Retry with exponential backoff and jitter for flaky network calls."""

from __future__ import annotations

import random
import time
from typing import Callable, TypeVar

import requests

T = TypeVar("T")

#: HTTP statuses worth retrying (rate limits and server errors).
RETRYABLE_STATUS = {408, 425, 429, 500, 502, 503, 504}


def is_retryable_status(status: int | None) -> bool:
    """Reports whether an HTTP status justifies another attempt."""
    return status is not None and status in RETRYABLE_STATUS


def is_retryable_error(exc: BaseException) -> bool:
    """Reports whether an exception justifies another attempt.

    Connection problems and timeouts are retried. HTTP errors retry only
    for retryable statuses; everything else (including client errors and
    programming mistakes) fails fast.
    """
    if isinstance(exc, requests.HTTPError):
        status = exc.response.status_code if exc.response is not None else None
        return is_retryable_status(status)
    return isinstance(exc, (requests.ConnectionError, requests.Timeout))


def with_retry(
    fn: Callable[[], T],
    attempts: int = 3,
    base_delay: float = 1.0,
    sleep: Callable[[float], None] = time.sleep,
    jitter: Callable[[float], float] = random.uniform,
) -> T:
    """Runs ``fn`` up to ``attempts`` times with backoff ``1s, 2s, 4s``.

    Each delay is jittered over ``[0, delay]`` so concurrent workers do not
    retry in lockstep. ``sleep`` and ``jitter`` are injectable so tests never
    wait on real clocks. The last error propagates unchanged.
    """
    if attempts < 1:
        raise ValueError("attempts must be at least 1")
    last_error: BaseException | None = None
    for attempt in range(attempts):
        try:
            return fn()
        except Exception as exc:  # noqa: BLE001 - policy is by exception type
            last_error = exc
            retryable = is_retryable_error(exc)
            if not retryable or attempt == attempts - 1:
                raise
            delay = base_delay * (2**attempt)
            sleep(jitter(0, delay))
    assert last_error is not None  # for type checkers; unreachable
    raise last_error
