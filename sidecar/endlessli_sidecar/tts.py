"""Text-to-speech engines for the sidecar.

Three engines, one JSON-RPC surface:

* ``piper``  local Piper voice model, invoked as a subprocess
* ``kokoro`` local Kokoro-ONNX model, imported when installed
* ``openai`` any OpenAI Speech-compatible ``/audio/speech`` endpoint

No new Python dependency is required. Kokoro needs the optional
``kokoro-onnx`` package plus model files, and its error message says so.
Audio is returned base64-encoded because the Rust side turns it into a
``data:`` URL for the webview.

Every path that comes from the UI is treated as untrusted: voice names must be
bare basenames and configured files must exist before anything is executed.
"""

from __future__ import annotations

import base64
import json
import subprocess
import tempfile
import urllib.error
import urllib.request
import wave
from array import array
from pathlib import Path
from typing import Any

#: Longest text accepted by one synthesis call.
MAX_TTS_CHARS = 8_000

#: Refuse oversized remote responses (25 MiB of audio).
MAX_AUDIO_BYTES = 25 * 1024 * 1024

#: Hard cap on a local engine run.
SUBPROCESS_TIMEOUT_SECS = 180


def _require_text(params: dict[str, Any]) -> str:
    text = params.get("text")
    if not isinstance(text, str) or not text.strip():
        raise ValueError("text must be a non-empty string")
    if len(text) > MAX_TTS_CHARS:
        raise ValueError(f"text exceeds the {MAX_TTS_CHARS}-character limit")
    return text.strip()


def _resolve_file(value: Any, label: str) -> Path:
    """Returns an existing file, rejecting blank and directory paths."""
    if not isinstance(value, str) or not value.strip():
        raise RuntimeError(f"{label} is not configured; set it in Settings > Audio")
    path = Path(value).expanduser()
    if not path.is_file():
        raise RuntimeError(f"{label} does not exist: {path}")
    return path


def _resolve_dir(value: Any, label: str) -> Path:
    if not isinstance(value, str) or not value.strip():
        raise RuntimeError(f"{label} is not configured; set it in Settings > Audio")
    path = Path(value).expanduser()
    if not path.is_dir():
        raise RuntimeError(f"{label} is not a directory: {path}")
    return path


def _safe_voice_name(voice: Any) -> str:
    """Rejects path traversal in a voice id."""
    if not isinstance(voice, str) or not voice.strip():
        raise ValueError("voice must be a non-empty string")
    name = voice.strip()
    if Path(name).name != name or name in {".", ".."}:
        raise ValueError("voice must be a bare voice name, not a path")
    return name


def _voice_file(voices_dir: Path, voice: str) -> Path:
    name = _safe_voice_name(voice)
    if not name.endswith(".onnx"):
        name = f"{name}.onnx"
    path = (voices_dir / name).resolve()
    if voices_dir.resolve() not in path.parents:
        raise ValueError("voice escapes the configured voices directory")
    if not path.is_file():
        raise RuntimeError(f"voice model not found: {path}")
    return path


def _wav_bytes(samples: Any, sample_rate: int) -> bytes:
    """Encodes mono float samples as 16-bit PCM WAV."""
    # kokoro-onnx returns a numpy float32 array; accept plain sequences too.
    if hasattr(samples, "tolist"):
        samples = samples.tolist()
    pcm = array(
        "h",
        (max(-32768, min(32767, int(round(float(sample) * 32767)))) for sample in samples),
    )
    with tempfile.TemporaryDirectory() as tmp:
        path = Path(tmp) / "speech.wav"
        with wave.open(str(path), "wb") as handle:
            handle.setnchannels(1)
            handle.setsampwidth(2)
            handle.setframerate(sample_rate)
            handle.writeframes(pcm.tobytes())
        data = path.read_bytes()
    if len(data) > MAX_AUDIO_BYTES:
        raise RuntimeError("synthesized audio is too large")
    return data


def _piper_synthesize(params: dict[str, Any], text: str) -> tuple[bytes, str]:
    binary = _resolve_file(params.get("piper_path"), "Piper executable")
    voices_dir = _resolve_dir(params.get("piper_voices_dir"), "Piper voices directory")
    voice_path = _voice_file(voices_dir, str(params.get("voice", "")))
    with tempfile.TemporaryDirectory() as tmp:
        output = Path(tmp) / "speech.wav"
        try:
            completed = subprocess.run(  # noqa: S603 - configured local binary
                [
                    str(binary),
                    "--model",
                    str(voice_path),
                    "--output_file",
                    str(output),
                ],
                input=text.encode("utf-8"),
                capture_output=True,
                timeout=SUBPROCESS_TIMEOUT_SECS,
                check=False,
            )
        except subprocess.TimeoutExpired as exc:
            raise RuntimeError("Piper timed out while synthesizing") from exc
        except OSError as exc:
            raise RuntimeError(f"could not start Piper: {exc}") from exc
        if completed.returncode != 0:
            detail = (completed.stderr or b"").decode("utf-8", "replace")[:300]
            raise RuntimeError(f"Piper failed (exit {completed.returncode}): {detail}")
        if not output.is_file():
            raise RuntimeError("Piper produced no audio file")
        data = output.read_bytes()
    if len(data) > MAX_AUDIO_BYTES:
        raise RuntimeError("synthesized audio is too large")
    return data, "audio/wav"


def _kokoro_voices_file(value: Any) -> Path:
    """Resolves the Kokoro voices archive, which must be the ``.bin`` file.

    kokoro-onnx loads this path with ``numpy.load``; the older JSON voice maps
    the app used to expect are not readable by the engine, so a stale path gets
    a message that says what to download instead of a cryptic NumPy error.
    """
    path = _resolve_file(value, "Kokoro voices")
    if path.suffix.lower() != ".bin":
        raise RuntimeError(
            "Kokoro voices must be the .bin archive shipped with the model "
            f"(got {path.name}); kokoro-onnx cannot read JSON voice files"
        )
    return path


def _kokoro_voice_names(voices_path: Path) -> list[str]:
    """Lists voice names inside the Kokoro ``.bin`` archive.

    The file is a NumPy ``.npz`` archive renamed to ``.bin``, and kokoro-onnx
    reads it with ``numpy.load`` and indexes it by voice name. Listing names
    through the same reader keeps the dropdown consistent with what the engine
    can actually synthesize.
    """
    try:
        import numpy as np  # type: ignore[import-not-found]
    except ImportError as exc:
        raise RuntimeError(
            "listing Kokoro voices needs numpy; install kokoro-onnx inside sidecar/"
        ) from exc
    try:
        archive = np.load(voices_path, allow_pickle=False)
    except (OSError, ValueError) as exc:
        raise RuntimeError(f"could not read Kokoro voices archive: {exc}") from exc
    try:
        names = getattr(archive, "files", None)
        if names is None:
            raise RuntimeError("Kokoro voices file must be a NumPy .npz archive")
        return sorted(str(name) for name in names)
    finally:
        close = getattr(archive, "close", None)
        if callable(close):
            close()


def _kokoro_synthesize(params: dict[str, Any], text: str) -> tuple[bytes, str]:
    try:
        from kokoro_onnx import Kokoro  # type: ignore[import-not-found]
    except ImportError as exc:
        raise RuntimeError(
            "Kokoro needs the optional 'kokoro-onnx' package and a local model. "
            "Install it with `uv pip install kokoro-onnx` inside sidecar/, or use Piper "
            "or an OpenAI-compatible speech endpoint instead."
        ) from exc
    model_path = _resolve_file(params.get("kokoro_model_path"), "Kokoro model")
    voices_path = _kokoro_voices_file(params.get("kokoro_voices_path"))
    voice = _safe_voice_name(params.get("voice"))
    try:
        # kokoro-onnx opens the voices archive itself, so the .bin path is
        # passed through unchanged rather than parsed here.
        engine = Kokoro(str(model_path), str(voices_path))
        samples, sample_rate = engine.create(text, voice=voice, speed=1.0, lang="en-us")
    except Exception as exc:  # noqa: BLE001 - surface engine errors to the UI
        raise RuntimeError(f"Kokoro synthesis failed: {exc}") from exc
    return _wav_bytes(samples, int(sample_rate)), "audio/wav"


def _openai_synthesize(params: dict[str, Any], text: str) -> tuple[bytes, str]:
    base_url = params.get("base_url")
    if not isinstance(base_url, str) or not base_url.strip():
        raise RuntimeError("speech endpoint base URL is not configured")
    base_url = base_url.strip().rstrip("/")
    if not base_url.startswith(("https://", "http://")):
        raise ValueError("speech endpoint must use http:// or https://")
    model = params.get("model")
    if not isinstance(model, str) or not model.strip():
        raise ValueError("speech endpoint model is required")
    voice = _safe_voice_name(params.get("voice"))
    body = {
        "model": model.strip(),
        "voice": voice,
        "input": text,
        "response_format": params.get("response_format", "mp3"),
    }
    headers = {"Content-Type": "application/json", "Accept": "audio/*"}
    api_key = params.get("api_key")
    if isinstance(api_key, str) and api_key.strip():
        headers["Authorization"] = f"Bearer {api_key.strip()}"
    request = urllib.request.Request(
        f"{base_url}/audio/speech",
        data=json.dumps(body).encode("utf-8"),
        headers=headers,
        method="POST",
    )
    try:
        with urllib.request.urlopen(request, timeout=SUBPROCESS_TIMEOUT_SECS) as response:  # noqa: S310 - configured endpoint
            data = response.read(MAX_AUDIO_BYTES + 1)
            content_type = response.headers.get_content_type()
    except urllib.error.HTTPError as exc:
        detail = exc.read(300).decode("utf-8", "replace") if exc.fp else ""
        raise RuntimeError(f"speech endpoint HTTP {exc.code}: {detail}") from exc
    except urllib.error.URLError as exc:
        raise RuntimeError(f"speech endpoint request failed: {exc.reason}") from exc
    if len(data) > MAX_AUDIO_BYTES:
        raise RuntimeError("speech endpoint returned more than 25 MiB")
    if not data:
        raise RuntimeError("speech endpoint returned no audio")
    if data[:4] == b"RIFF":
        return data, "audio/wav"
    if content_type.startswith("audio/"):
        return data, content_type
    return data, "audio/mpeg"


def synthesize(params: Any) -> dict[str, Any]:
    """Routes one synthesis request and returns base64 audio."""
    if not isinstance(params, dict):
        raise ValueError("params must be an object")
    engine = params.get("engine")
    if not isinstance(engine, str) or not engine.strip():
        raise ValueError("engine must be a non-empty string")
    text = _require_text(params)
    match engine.strip().lower():
        case "piper":
            data, mime = _piper_synthesize(params, text)
        case "kokoro":
            data, mime = _kokoro_synthesize(params, text)
        case "openai":
            data, mime = _openai_synthesize(params, text)
        case other:
            raise ValueError(f"unknown TTS engine: {other}")
    return {
        "audio_base64": base64.b64encode(data).decode("ascii"),
        "mime": mime,
        "bytes": len(data),
    }


def list_voices(params: Any) -> dict[str, Any]:
    """Lists voices for a local engine.

    Remote endpoints keep their voice list in Settings, so this only handles
    files on disk.
    """
    if not isinstance(params, dict):
        raise ValueError("params must be an object")
    engine = params.get("engine")
    if engine == "piper":
        directory = _resolve_dir(params.get("voices_dir"), "Piper voices directory")
        voices = sorted(
            path.stem for path in directory.glob("*.onnx") if path.is_file()
        )
        return {"voices": voices}
    if engine == "kokoro":
        path = _kokoro_voices_file(params.get("voices_path"))
        return {"voices": _kokoro_voice_names(path)}
    raise ValueError(f"voice listing is not supported for engine: {engine}")
