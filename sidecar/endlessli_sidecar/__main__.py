"""Sidecar entrypoint: ``python -m endlessli_sidecar`` serves on stdio."""

from .server import serve

if __name__ == "__main__":
    raise SystemExit(serve())
