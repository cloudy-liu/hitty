#!/usr/bin/env python3
"""Check the complete public asset set before publication or after download."""

import argparse
import hashlib
from pathlib import Path
import re


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    parser.add_argument("version")
    parser.add_argument("--checksums", action="store_true", help="also verify checksums.txt")
    args = parser.parse_args()
    if not re.fullmatch(r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)", args.version):
        parser.error("version must be X.Y.Z")
    expected = {
        f"ctty7-{args.version}-{suffix}"
        for suffix in (
            "windows-x86_64-setup.exe", "windows-x86_64.zip",
            "linux-x86_64.AppImage", "linux-x86_64.tar.gz",
            "macos-arm64.dmg", "macos-arm64.zip",
            "macos-x86_64.dmg", "macos-x86_64.zip",
        )
    } | {
        "tty7-server-linux-x86_64-musl", "tty7-server-linux-aarch64-musl",
        "tty7-server-macos-aarch64", "tty7-server-macos-x86_64",
    }
    actual = {p.name for p in args.directory.iterdir()}
    allowed = expected | ({"checksums.txt"} if args.checksums else set())
    if actual != allowed:
        parser.error(f"missing: {sorted(allowed - actual)}; unexpected: {sorted(actual - allowed)}")
    for name in expected:
        path = args.directory / name
        if not path.is_file() or path.is_symlink() or path.stat().st_size == 0:
            parser.error(f"asset is not a nonempty regular file: {name}")
    if args.checksums:
        entries = {}
        for line in (args.directory / "checksums.txt").read_text(encoding="utf-8").splitlines():
            match = re.fullmatch(r"([0-9a-fA-F]{64})  (.+)", line)
            if not match or match[2] in entries:
                parser.error(f"invalid or duplicate checksum entry: {line}")
            entries[match[2]] = match[1].lower()
        if set(entries) != expected:
            parser.error("checksum entries must match the complete program asset set")
        for name, expected_digest in entries.items():
            digest = hashlib.sha256()
            with (args.directory / name).open("rb") as asset:
                for block in iter(lambda: asset.read(1024 * 1024), b""):
                    digest.update(block)
            if digest.hexdigest() != expected_digest:
                parser.error(f"checksum mismatch: {name}")
    print(f"Verified {len(expected)} program assets" + (" and checksums" if args.checksums else ""))


if __name__ == "__main__":
    main()
