#!/usr/bin/env python3
"""Create a new Ed25519 firmware signing key at an explicitly selected path."""

import argparse
import os
from pathlib import Path


def write_private_key(path, pem):
    path.parent.mkdir(parents=True, exist_ok=True)
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    try:
        with os.fdopen(fd, "wb") as output:
            output.write(pem)
        path.chmod(0o600)
    except BaseException:
        path.unlink(missing_ok=True)
        raise


def main(argv=None):
    parser = argparse.ArgumentParser(description="Generate an Ed25519 OTA signing key")
    parser.add_argument("--key", required=True, type=Path, help="New private key PEM path")
    args = parser.parse_args(argv)
    path = args.key.expanduser()
    if path.exists():
        parser.exit(1, f"Error: key already exists at {path}\n")
    try:
        from cryptography.hazmat.primitives import serialization
        from cryptography.hazmat.primitives.asymmetric import ed25519
    except ImportError:
        parser.exit(1, "Error: key generation requires the optional cryptography package\n")
    private_key = ed25519.Ed25519PrivateKey.generate()
    pem = private_key.private_bytes(
        encoding=serialization.Encoding.PEM,
        format=serialization.PrivateFormat.PKCS8,
        encryption_algorithm=serialization.NoEncryption(),
    )
    try:
        write_private_key(path, pem)
    except FileExistsError:
        parser.exit(1, f"Error: key already exists at {path}\n")
    public = private_key.public_key().public_bytes(
        encoding=serialization.Encoding.Raw,
        format=serialization.PublicFormat.Raw,
    )
    rust_bytes = ", ".join(f"0x{byte:02x}" for byte in public)
    print(f"Generated new Ed25519 private key: {path}")
    print(f"pub const OTA_PUBLIC_KEY: [u8; 32] = [{rust_bytes}];")


if __name__ == "__main__":
    main()
