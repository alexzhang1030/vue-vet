#!/usr/bin/env python3
"""Measure the host release `vue-vet` file size and a reproducible gzip-9 proxy.

gzip-9 here is a comparable compression proxy of the stripped native binary
(mtime 0, no stored filename). It is not GitHub release-archive or npm tarball
bytes.

Build mode (default) resolves the executable from Cargo `--message-format=json`
compiler-artifact records. Artifact mode (`--binary` + `--target`) measures an
already-built file and never falls back to cargo.
"""

from __future__ import annotations

import argparse
import gzip
import io
import json
import stat
import subprocess
import sys
from pathlib import Path
from typing import Any

EXIT_OVER_BUDGET = 1
EXIT_OPERATIONAL = 2
EXIT_NO_CARGO = 127

KNOWN_TARGETS = (
  "x86_64-unknown-linux-gnu",
  "aarch64-unknown-linux-gnu",
  "x86_64-apple-darwin",
  "aarch64-apple-darwin",
  "x86_64-pc-windows-msvc",
)


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
        sys.stderr.write(
            "cargo not found on PATH; install the pinned Rust toolchain before just native-size\n"
        )
        return EXIT_NO_CARGO, ""
    return proc.returncode, proc.stdout


def measure_binary(path: Path) -> dict[str, Any]:
    data = path.read_bytes()
    return {
        "binary": str(path),
        "file_bytes": len(data),
        "gzip9_bytes": gzip9_bytes(data),
    }


def operational(message: str) -> int:
    sys.stderr.write(message if message.endswith("\n") else f"{message}\n")
    return EXIT_OPERATIONAL


def require_regular_file(path: Path) -> str | None:
    try:
        st = path.stat()
    except FileNotFoundError:
        return f"artifact not found: {path}"
    except OSError as error:
        return f"cannot stat artifact {path}: {error}"
    if not stat.S_ISREG(st.st_mode):
        return f"artifact is not a regular file: {path}"
    if st.st_size <= 0:
        return f"artifact is empty: {path}"
    return None


def load_target_budget(budget_file: Path, target: str) -> dict[str, int] | str:
    if not budget_file.is_file():
        return f"budget file not found: {budget_file}"
    try:
        payload = json.loads(budget_file.read_text(encoding="utf-8"))
    except UnicodeError as error:
        return f"budget file is not valid UTF-8: {budget_file}: {error}"
    except OSError as error:
        return f"cannot read budget file {budget_file}: {error}"
    except json.JSONDecodeError as error:
        return f"budget file is not valid JSON: {budget_file}: {error}"
    if not isinstance(payload, dict):
        return f"budget file must be a JSON object: {budget_file}"
    targets = payload.get("targets")
    if not isinstance(targets, dict):
        return f"budget file missing object 'targets': {budget_file}"
    entry = targets.get(target)
    if entry is None:
        known = ", ".join(sorted(targets)) or "(none)"
        return f"unknown target {target!r}; budget entries: {known}"
    if not isinstance(entry, dict):
        return f"budget entry for {target} must be an object"
    out: dict[str, int] = {}
    for key in ("file_bytes", "gzip9_bytes"):
        value = entry.get(key)
        if not isinstance(value, int) or isinstance(value, bool) or value <= 0:
            return f"budget {target}.{key} must be a positive integer, got {value!r}"
        out[key] = value
    return out


def evaluate_budget(measured: dict[str, Any], budget: dict[str, int]) -> list[str]:
    failures: list[str] = []
    file_bytes = int(measured["file_bytes"])
    gzip9 = int(measured["gzip9_bytes"])
    if file_bytes > budget["file_bytes"]:
        failures.append(f"file_bytes {file_bytes} exceeds budget {budget['file_bytes']}")
    if gzip9 > budget["gzip9_bytes"]:
        failures.append(f"gzip9_bytes {gzip9} exceeds budget {budget['gzip9_bytes']}")
    return failures


def emit_report(report: dict[str, Any], *, as_json: bool) -> None:
    if as_json:
        sys.stdout.write(json.dumps(report, indent=2, sort_keys=True) + "\n")
        return
    print(f"binary={report['binary']}")
    print(f"file_bytes={report['file_bytes']}")
    print(f"gzip9_bytes={report['gzip9_bytes']}")
    if "target" in report:
        print(f"target={report['target']}")


def measure_existing_artifact(binary: Path, target: str, budget_file: Path | None) -> tuple[dict[str, Any], int]:
    if target not in KNOWN_TARGETS:
        return {}, operational(
            f"unrecognized target {target!r}; expected one of {', '.join(KNOWN_TARGETS)}"
        )
    problem = require_regular_file(binary)
    if problem:
        return {}, operational(problem)
    try:
        report = measure_binary(binary)
    except OSError as error:
        return {}, operational(f"cannot read artifact {binary}: {error}")
    report["target"] = target
    report["ok"] = True
    if budget_file is None:
        return report, 0
    budget = load_target_budget(budget_file, target)
    if isinstance(budget, str):
        return {}, operational(budget)
    report["budget"] = budget
    failures = evaluate_budget(report, budget)
    if failures:
        report["ok"] = False
        report["failures"] = failures
        for item in failures:
            sys.stderr.write(f"{item}\n")
        return report, EXIT_OVER_BUDGET
    return report, 0


def parse_args(argv: list[str] | None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--root",
        type=Path,
        default=None,
        help="workspace root for build mode (default: parent of this script)",
    )
    parser.add_argument(
        "--binary",
        type=Path,
        default=None,
        help="already-built vue-vet executable (artifact mode; never falls back to cargo)",
    )
    parser.add_argument(
        "--target",
        default=None,
        help="Rust target triple matching the budget table (required with --binary)",
    )
    parser.add_argument(
        "--budget-file",
        type=Path,
        default=None,
        help="JSON budget table with per-target file_bytes and gzip9_bytes maxima",
    )
    parser.add_argument(
        "--json",
        action="store_true",
        help="print a JSON measurement object on stdout",
    )
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    artifact_requested = args.binary is not None or args.target is not None
    if args.budget_file is not None and not artifact_requested:
        return operational("--budget-file requires --binary and --target (will not fall back to cargo)")
    if artifact_requested:
        if args.binary is None or args.target is None:
            return operational("artifact mode requires both --binary PATH and --target RUST_TRIPLE")
        report, code = measure_existing_artifact(args.binary, args.target, args.budget_file)
        if code == EXIT_OPERATIONAL:
            return code
        emit_report(report, as_json=args.json)
        return code

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
        return EXIT_OPERATIONAL
    problem = require_regular_file(binary)
    if problem:
        return operational(problem)
    try:
        report = measure_binary(binary)
    except OSError as error:
        return operational(f"cannot read artifact {binary}: {error}")
    emit_report(report, as_json=args.json)
    return 0


if __name__ == "__main__":
    sys.exit(main())
