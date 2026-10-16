"""Server dispatch: protocol shape, validation, and error mapping."""

import io
import json

from endlessli_sidecar import server
from endlessli_sidecar.server import dispatch, serve


def test_health_check():
    response = dispatch({"jsonrpc": "2.0", "id": 1, "method": "health.check", "params": {}})
    assert response["id"] == 1
    assert response["result"]["ok"] is True
    assert response["result"]["version"]


def test_search_providers_lists_backends():
    response = dispatch({"jsonrpc": "2.0", "id": 2, "method": "search.providers", "params": {}})
    names = {entry["name"]: entry["requires_key"] for entry in response["result"]["providers"]}
    assert names == {"ddgs": False, "searxng": False, "brave": True}


def test_unknown_method():
    response = dispatch({"jsonrpc": "2.0", "id": 3, "method": "nope", "params": {}})
    assert response["error"]["code"] == server.METHOD_NOT_FOUND
    assert response["id"] == 3


def test_invalid_params_mapped():
    response = dispatch({"jsonrpc": "2.0", "id": 4, "method": "clip.url", "params": {"url": "http://x.test"}})
    assert response["error"]["code"] == server.INVALID_PARAMS


def test_non_object_request_rejected():
    response = dispatch([1, 2, 3])
    assert response["error"]["code"] == server.INVALID_REQUEST


def test_serve_handles_bad_json_and_eof():
    stdin = io.StringIO('not json\n{"jsonrpc": "2.0", "id": 9, "method": "health.check", "params": {}}\n')
    stdout = io.StringIO()
    assert serve(stdin, stdout) == 0
    lines = [json.loads(line) for line in stdout.getvalue().splitlines()]
    assert lines[0]["error"]["code"] == server.PARSE_ERROR
    assert lines[1]["result"]["ok"] is True
