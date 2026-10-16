"""Retry policy: backoff schedule, jitter, and fail-fast behavior."""

import requests

from endlessli_sidecar.retry import with_retry


def test_succeeds_after_transient_failures():
    sleeps = []
    attempts = {"n": 0}

    def flaky():
        attempts["n"] += 1
        if attempts["n"] < 3:
            raise requests.ConnectionError("boom")
        return "ok"

    result = with_retry(flaky, sleep=sleeps.append, jitter=lambda lo, hi: hi)
    assert result == "ok"
    assert attempts["n"] == 3
    # Exponential backoff: 1s, then 2s.
    assert sleeps == [1.0, 2.0]


def test_gives_up_after_attempts():
    sleeps = []

    def always_down():
        raise requests.Timeout("slow")

    try:
        with_retry(always_down, attempts=3, sleep=sleeps.append, jitter=lambda lo, hi: hi)
    except requests.Timeout:
        pass
    else:  # pragma: no cover - must raise
        raise AssertionError("expected Timeout")
    assert len(sleeps) == 2


def test_client_errors_fail_fast_without_sleeping():
    sleeps = []
    calls = {"n": 0}

    def bad_request():
        calls["n"] += 1
        response = requests.Response()
        response.status_code = 400
        raise requests.HTTPError("bad", response=response)

    try:
        with_retry(bad_request, sleep=sleeps.append)
    except requests.HTTPError:
        pass
    else:  # pragma: no cover - must raise
        raise AssertionError("expected HTTPError")
    assert calls["n"] == 1
    assert sleeps == []


def test_rate_limit_retries():
    sleeps = []

    def limited():
        response = requests.Response()
        response.status_code = 429
        raise requests.HTTPError("slow down", response=response)

    try:
        with_retry(limited, attempts=2, sleep=sleeps.append, jitter=lambda lo, hi: hi)
    except requests.HTTPError:
        pass
    else:  # pragma: no cover - must raise
        raise AssertionError("expected HTTPError")
    assert len(sleeps) == 1


def test_zero_attempts_rejected():
    try:
        with_retry(lambda: None, attempts=0)
    except ValueError:
        pass
    else:  # pragma: no cover - must raise
        raise AssertionError("expected ValueError")
