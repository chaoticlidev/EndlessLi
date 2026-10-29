"""TTS engines: path safety, local voice listing, and remote synthesis."""

import base64
import email.message
import io

import pytest

from endlessli_sidecar import tts


class FakeHeaders(email.message.Message):
    pass


class FakeResponse:
    def __init__(self, data: bytes, content_type: str):
        self.data = data
        headers = FakeHeaders()
        headers.add_header("Content-Type", content_type)
        self.headers = headers

    def __enter__(self):
        return self

    def __exit__(self, *args):
        return False

    def read(self, limit: int = -1) -> bytes:
        if limit < 0:
            return self.data
        return self.data[:limit]


def test_piper_lists_onnx_voices(tmp_path):
    (tmp_path / "en_US-lessac-medium.onnx").write_bytes(b"x")
    (tmp_path / "en_GB-alba-medium.onnx").write_bytes(b"x")
    (tmp_path / "notes.txt").write_text("ignored")
    result = tts.list_voices({"engine": "piper", "voices_dir": str(tmp_path)})
    assert result["voices"] == ["en_GB-alba-medium", "en_US-lessac-medium"]


def test_piper_missing_directory_is_actionable(tmp_path):
    with pytest.raises(RuntimeError, match="not a directory"):
        tts.list_voices({"engine": "piper", "voices_dir": str(tmp_path / "missing")})


def test_voice_path_traversal_rejected(tmp_path):
    binary = tmp_path / "piper"
    binary.write_text("stub")
    voices = tmp_path / "voices"
    voices.mkdir()
    with pytest.raises(ValueError, match="bare voice name"):
        tts.synthesize(
            {
                "engine": "piper",
                "text": "hello",
                "piper_path": str(binary),
                "piper_voices_dir": str(voices),
                "voice": "../escape",
            }
        )


def test_missing_piper_binary_is_actionable(tmp_path):
    with pytest.raises(RuntimeError, match="Piper executable"):
        tts.synthesize(
            {
                "engine": "piper",
                "text": "hello",
                "piper_path": str(tmp_path / "nope"),
                "piper_voices_dir": str(tmp_path),
                "voice": "en_US-lessac-medium",
            }
        )


def test_openai_speech_returns_wav_data(monkeypatch):
    wav = b"RIFF" + b"\x00" * 32
    captured = {}

    def fake_urlopen(request, timeout):
        captured["url"] = request.full_url
        captured["body"] = request.data
        captured["auth"] = request.headers.get("Authorization")
        return FakeResponse(wav, "application/octet-stream")

    monkeypatch.setattr(tts.urllib.request, "urlopen", fake_urlopen)
    result = tts.synthesize(
        {
            "engine": "openai",
            "text": "hello",
            "voice": "alloy",
            "model": "tts-1",
            "base_url": "https://speech.test/v1",
            "api_key": "secret",
        }
    )
    assert captured["url"] == "https://speech.test/v1/audio/speech"
    assert captured["auth"] == "Bearer secret"
    assert b'"input": "hello"' in captured["body"]
    assert result["mime"] == "audio/wav"
    assert base64.b64decode(result["audio_base64"]) == wav
    assert result["bytes"] == len(wav)


def test_openai_speech_requires_http_url():
    with pytest.raises(ValueError, match="http"):
        tts.synthesize(
            {
                "engine": "openai",
                "text": "hello",
                "voice": "alloy",
                "model": "tts-1",
                "base_url": "file:///tmp/speech",
            }
        )


def test_openai_speech_requires_model():
    with pytest.raises(ValueError, match="model"):
        tts.synthesize(
            {
                "engine": "openai",
                "text": "hello",
                "voice": "alloy",
                "base_url": "https://speech.test/v1",
            }
        )


def test_kokoro_reports_missing_dependency_or_model(tmp_path):
    with pytest.raises(RuntimeError, match="Kokoro"):
        tts.synthesize(
            {
                "engine": "kokoro",
                "text": "hello",
                "voice": "af_heart",
                "kokoro_model_path": str(tmp_path / "model.onnx"),
                "kokoro_voices_path": str(tmp_path / "voices-v1.0.bin"),
            }
        )


def test_kokoro_lists_voices_from_bin_archive(tmp_path):
    """The .bin file is a NumPy .npz archive; voice names are its keys."""
    numpy = pytest.importorskip("numpy")
    archive_path = tmp_path / "voices-v1.0.bin"
    with open(archive_path, "wb") as handle:
        numpy.savez(handle, af_bella=numpy.zeros(4), af_heart=numpy.zeros(4))

    result = tts.list_voices({"engine": "kokoro", "voices_path": str(archive_path)})
    assert result["voices"] == ["af_bella", "af_heart"]


def test_kokoro_rejects_json_voices_file(tmp_path):
    """Older settings stored a .json voices map, which kokoro-onnx cannot read."""
    path = tmp_path / "voices.json"
    path.write_text('{"af_heart": []}', encoding="utf-8")
    with pytest.raises(RuntimeError, match="must be the .bin archive"):
        tts.list_voices({"engine": "kokoro", "voices_path": str(path)})


def test_kokoro_rejects_non_archive_bin(tmp_path):
    path = tmp_path / "voices.bin"
    path.write_bytes(b"not an npz archive")
    with pytest.raises(RuntimeError, match="could not read Kokoro voices archive"):
        tts.list_voices({"engine": "kokoro", "voices_path": str(path)})


def test_text_limit_enforced():
    with pytest.raises(ValueError, match="8000"):
        tts.synthesize({"engine": "piper", "text": "a" * 8001})


def test_unknown_engine_rejected():
    with pytest.raises(ValueError, match="unknown TTS engine"):
        tts.synthesize({"engine": "espeak", "text": "hello"})


def test_wav_encoding_roundtrip():
    data = tts._wav_bytes([0.0, 0.5, -0.5], 24_000)
    assert data[:4] == b"RIFF"
    import wave

    with wave.open(io.BytesIO(data), "rb") as handle:
        assert handle.getframerate() == 24_000
        assert handle.getnchannels() == 1
        assert handle.getsampwidth() == 2
