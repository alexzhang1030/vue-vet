#!/usr/bin/env python3
"""Measure the host release `vue-vet` file size and a reproducible gzip-9 proxy.

Resolves the executable from Cargo `--message-format=json` compiler-artifact
records (not by concatenating CARGO_TARGET_DIR and the rustc host triple).
Does not assert machine-specific byte counts.
"""

from __future__ import annotations

import argparse
import gzip
import io
import json
import subprocess
import sys
from pathlib import Path


def gzip9_bytes(data: bytes) -> int:
    """gzip-9 payload with mtime 0 and no stored filename."""
    buf = io.BytesIO()
    with gzip.GzipFile(filename="", mode="wb", fileobj=buf, compresslevel=9, mtime=0) as gz:
        gz.write(data)
    return len(buf.getvalue())


def executable_from_cargo_messages(lines: list[str]) -> Path:
    """Pick the vue-vet bin executable from cargo JSON lines."""
    found: list[Path] = []
    for raw in lines:
        raw = raw.strip()
        if not raw.startswith("{"):
            continue
        try:
            msg = json.loads(raw)
        except json.JSONDecodeError:
            continue
        if msg.get("reason") != "compiler-artifact":
            continue
        target = msg.get("target") or {}
        kinds = target.get("kind") or []
        if "bin" not in kinds:
            continue
        if target.get("name") != "vue-vet":
            continue
        if msg.get("profile", {}).get("test"):
            continue
        executable = msg.get("executable")
        if not executable:
            continue
        found.append(Path(executable))
    if not found:
        raise RuntimeError("cargo JSON had no vue-vet bin executable artifact")
    return found[-1]


def rendered_compiler_messages(lines: list[str]) -> list[str]:
    """Collect rustc `message.rendered` text from cargo JSON compiler-message records."""
    rendered: list[str] = []
    for raw in lines:
        raw = raw.strip()
        if not raw.startswith("{"):
            continue
        try:
            msg = json.loads(raw)
        except json.JSONDecodeError:
            continue
        if msg.get("reason") != "compiler-message":
            continue
        text = (msg.get("message") or {}).get("rendered")
        if isinstance(text, str) and text:
            rendered.append(text if text.endswith("\n") else f"{text}\n")
    return rendered


def forward_compiler_messages(stdout: str) -> None:
    for text in rendered_compiler_messages(stdout.splitlines()):
        sys.stderr.write(text)


def run_release_build(root: Path) -> tuple[int, str]:
    """Run the release build. JSON artifacts stay on stdout; cargo stderr is inherited."""
    try:
        proc = subprocess.run(
            ["cargo", "build", "-p", "vue-vet", "--release", "--locked", "--message-format=json"],
            cwd=root,
            stdout=subprocess.PIPE,
            stderr=None,
            text=True,
        )
    except FileNotFoundError:
        sys.stderr.write("cargo not found on PATH; install the pinned Rust toolchain before just native-size\n")
        return 127, ""
    return proc.returncode, proc.stdout


def measure_binary(path: Path) -> dict[str, object]:
    data = path.read_bytes()
    return {
        "binary": str(path),
        "file_bytes": len(data),
        "gzip9_bytes": gzip9_bytes(data),
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--root",
        type=Path,
        default=None,
        help="workspace root (default: parent of this script)",
    )
    args = parser.parse_args(argv)
    root = (args.root or Path(__file__).resolve().parent.parent).resolve()
    code, stdout = run_release_build(root)
    forward_compiler_messages(stdout)
    if code != 0:
        sys.stderr.write("cargo build -p vue-vet --release --locked failed\n")
        return code
    try:
        binary = executable_from_cargo_messages(stdout.splitlines())
    except RuntimeError as error:
        sys.stderr.write(f"{error}\n")
        return 1
    if not binary.is_file():
        sys.stderr.write(f"artifact is not a file: {binary}\n")
        return 1
    report = measure_binary(binary)
    print(f"binary={report['binary']}")
    print(f"file_bytes={report['file_bytes']}")
    print(f"gzip9_bytes={report['gzip9_bytes']}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
