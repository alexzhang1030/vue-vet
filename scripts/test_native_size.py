#!/usr/bin/env python3
"""Focused checks for scripts/native_size.py (no fat-LTO release build)."""

from __future__ import annotations

import json
import os
import stat
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPT_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(SCRIPT_DIR))
import native_size  # noqa: E402


def cargo_artifact_line(executable: str) -> str:
    return json.dumps(
        {
            "reason": "compiler-artifact",
            "package_id": "path+file:///tmp/vue-vet#vue-vet@0.1.23",
            "target": {"name": "vue-vet", "kind": ["bin"], "crate_types": ["bin"]},
            "profile": {"test": False},
            "executable": executable,
        }
    )


class GzipRepro(unittest.TestCase):
    def test_mtime_and_empty_name_are_stable(self) -> None:
        payload = b"vue-vet-native-size"
        first = native_size.gzip9_bytes(payload)
        second = native_size.gzip9_bytes(payload)
        self.assertEqual(first, second)
        raw = __import__("io").BytesIO()
        import gzip

        with gzip.GzipFile(filename="", mode="wb", fileobj=raw, compresslevel=9, mtime=0) as gz:
            gz.write(payload)
        blob = raw.getvalue()
        self.assertEqual(blob[4:8], b"\x00\x00\x00\x00")  # mtime
        flags = blob[3]
        self.assertEqual(flags & 0x08, 0)  # no FNAME


class ArtifactParse(unittest.TestCase):
    def test_picks_bin_executable_not_rlib(self) -> None:
        lines = [
            json.dumps({"reason": "compiler-artifact", "target": {"name": "vue_vet_core", "kind": ["lib"]}, "executable": None}),
            cargo_artifact_line("/tmp/custom dir/release/vue-vet"),
        ]
        self.assertEqual(
            native_size.executable_from_cargo_messages(lines),
            Path("/tmp/custom dir/release/vue-vet"),
        )

    def test_ignores_test_profile_bins(self) -> None:
        test_line = json.dumps(
            {
                "reason": "compiler-artifact",
                "target": {"name": "vue-vet", "kind": ["bin"]},
                "profile": {"test": True},
                "executable": "/tmp/deps/vue-vet-test",
            }
        )
        lines = [test_line, cargo_artifact_line("/tmp/real/vue-vet")]
        self.assertEqual(native_size.executable_from_cargo_messages(lines), Path("/tmp/real/vue-vet"))

    def test_missing_artifact_errors(self) -> None:
        with self.assertRaises(RuntimeError):
            native_size.executable_from_cargo_messages(["not json", "{}"])

    def test_rendered_compiler_messages(self) -> None:
        lines = [
            json.dumps(
                {
                    "reason": "compiler-message",
                    "message": {"rendered": "error: NATIVE_SIZE_RENDERED_SENTINEL\n", "level": "error"},
                }
            ),
            cargo_artifact_line("/tmp/real/vue-vet"),
        ]
        self.assertEqual(
            native_size.rendered_compiler_messages(lines),
            ["error: NATIVE_SIZE_RENDERED_SENTINEL\n"],
        )


class ScriptInvocation(unittest.TestCase):
    def _fake_cargo(
        self,
        directory: Path,
        *,
        executable: Path | None,
        exit_code: int,
        stderr_line: str | None = None,
        stdout_json: str | None = None,
    ) -> Path:
        cargo = directory / "cargo"
        stdout_lines: list[str] = []
        if stdout_json is not None:
            stdout_lines.append(stdout_json)
        if executable is None:
            stdout_lines.append(
                json.dumps({"reason": "compiler-artifact", "target": {"name": "other", "kind": ["lib"]}})
            )
        else:
            stdout_lines.append(cargo_artifact_line(str(executable)))
        stub = {
            "stderr_line": stderr_line,
            "stdout_lines": stdout_lines,
            "exit_code": exit_code,
        }
        cargo.write_text(
            "#!/usr/bin/env python3\n"
            "import json, sys\n"
            f"stub = json.loads({json.dumps(json.dumps(stub))})\n"
            "if stub['stderr_line']:\n"
            "    sys.stderr.write(stub['stderr_line'] + '\\n')\n"
            "for line in stub['stdout_lines']:\n"
            "    sys.stdout.write(line + '\\n')\n"
            "raise SystemExit(stub['exit_code'])\n"
        )
        cargo.chmod(cargo.stat().st_mode | stat.S_IEXEC)
        return cargo

    def test_custom_target_dir_and_spaces_in_path(self) -> None:
        with tempfile.TemporaryDirectory(prefix="native size ") as raw:
            root = Path(raw)
            bindir = root / "target dir" / "release"
            bindir.mkdir(parents=True)
            binary = bindir / "vue-vet"
            payload = b"\x7fELFfake"
            binary.write_bytes(payload)
            cargo_dir = root / "bin"
            cargo_dir.mkdir()
            self._fake_cargo(cargo_dir, executable=binary, exit_code=0)
            env = os.environ.copy()
            env["PATH"] = f"{cargo_dir}{os.pathsep}{env.get('PATH', '')}"
            env["CARGO_TARGET_DIR"] = str(root / "ignored-stale")
            proc = subprocess.run(
                [sys.executable, str(SCRIPT_DIR / "native_size.py"), "--root", str(root)],
                capture_output=True,
                text=True,
                env=env,
                cwd=root,
            )
            self.assertEqual(proc.returncode, 0, proc.stderr)
            self.assertIn(f"binary={binary}", proc.stdout)
            self.assertIn(f"file_bytes={len(payload)}", proc.stdout)
            self.assertIn("gzip9_bytes=", proc.stdout)

    def test_failed_build_does_not_use_stale_binary(self) -> None:
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw)
            stale = root / "target" / "release"
            stale.mkdir(parents=True)
            (stale / "vue-vet").write_bytes(b"stale")
            cargo_dir = root / "bin"
            cargo_dir.mkdir()
            sentinel = "NATIVE_SIZE_SENTINEL_ERROR"
            self._fake_cargo(
                cargo_dir,
                executable=stale / "vue-vet",
                exit_code=7,
                stderr_line=sentinel,
            )
            env = os.environ.copy()
            env["PATH"] = f"{cargo_dir}{os.pathsep}{env.get('PATH', '')}"
            proc = subprocess.run(
                [sys.executable, str(SCRIPT_DIR / "native_size.py"), "--root", str(root)],
                capture_output=True,
                text=True,
                env=env,
            )
            self.assertEqual(proc.returncode, 7)
            self.assertIn(sentinel, proc.stderr)
            self.assertIn("failed", proc.stderr.lower())

    def test_failed_build_forwards_compiler_message_rendered(self) -> None:
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw)
            cargo_dir = root / "bin"
            cargo_dir.mkdir()
            rendered = "error: NATIVE_SIZE_JSON_RENDERED_SENTINEL"
            compiler_line = json.dumps(
                {
                    "reason": "compiler-message",
                    "message": {"rendered": f"{rendered}\n", "level": "error"},
                }
            )
            self._fake_cargo(
                cargo_dir,
                executable=None,
                exit_code=101,
                stdout_json=compiler_line,
            )
            env = os.environ.copy()
            env["PATH"] = f"{cargo_dir}{os.pathsep}{env.get('PATH', '')}"
            proc = subprocess.run(
                [sys.executable, str(SCRIPT_DIR / "native_size.py"), "--root", str(root)],
                capture_output=True,
                text=True,
                env=env,
            )
            self.assertEqual(proc.returncode, 101)
            self.assertIn(rendered, proc.stderr)
            self.assertIn("failed", proc.stderr.lower())

    def test_successful_build_forwards_compiler_warning_rendered(self) -> None:
        with tempfile.TemporaryDirectory(prefix="native size warn ") as raw:
            root = Path(raw)
            bindir = root / "release"
            bindir.mkdir(parents=True)
            binary = bindir / "vue-vet"
            binary.write_bytes(b"\x7fELFfake")
            cargo_dir = root / "bin"
            cargo_dir.mkdir()
            warning = "warning: NATIVE_SIZE_JSON_WARNING_SENTINEL"
            compiler_line = json.dumps(
                {
                    "reason": "compiler-message",
                    "message": {"rendered": f"{warning}\n", "level": "warning"},
                }
            )
            self._fake_cargo(
                cargo_dir,
                executable=binary,
                exit_code=0,
                stdout_json=compiler_line,
            )
            env = os.environ.copy()
            env["PATH"] = f"{cargo_dir}{os.pathsep}{env.get('PATH', '')}"
            proc = subprocess.run(
                [sys.executable, str(SCRIPT_DIR / "native_size.py"), "--root", str(root)],
                capture_output=True,
                text=True,
                env=env,
            )
            self.assertEqual(proc.returncode, 0, proc.stderr)
            self.assertIn(warning, proc.stderr)
            self.assertIn(f"binary={binary}", proc.stdout)

    def test_successful_build_without_executable_fails(self) -> None:
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw)
            cargo_dir = root / "bin"
            cargo_dir.mkdir()
            self._fake_cargo(cargo_dir, executable=None, exit_code=0)
            env = os.environ.copy()
            env["PATH"] = f"{cargo_dir}{os.pathsep}{env.get('PATH', '')}"
            proc = subprocess.run(
                [sys.executable, str(SCRIPT_DIR / "native_size.py"), "--root", str(root)],
                capture_output=True,
                text=True,
                env=env,
            )
            self.assertEqual(proc.returncode, 1)
            self.assertIn("no vue-vet bin executable", proc.stderr)

    def test_missing_cargo_is_operational(self) -> None:
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw)
            env = os.environ.copy()
            env["PATH"] = str(root / "empty-bin")
            (root / "empty-bin").mkdir()
            proc = subprocess.run(
                [sys.executable, str(SCRIPT_DIR / "native_size.py"), "--root", str(root)],
                capture_output=True,
                text=True,
                env=env,
            )
            self.assertEqual(proc.returncode, 127)
            self.assertIn("cargo not found", proc.stderr)
            self.assertNotIn("Traceback", proc.stderr)


if __name__ == "__main__":
    unittest.main()
