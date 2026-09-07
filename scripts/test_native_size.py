#!/usr/bin/env python3
"""Focused checks for scripts/native_size.py (no fat-LTO release build)."""

from __future__ import annotations

import gzip
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
    def test_repeated_payload_compresses_and_matches_stdlib_compress(self) -> None:
        payload = b"A" * 50_000
        got = native_size.gzip9_bytes(payload)
        independent = len(gzip.compress(payload, compresslevel=9, mtime=0))
        self.assertEqual(got, independent)
        self.assertLess(got, 128, "50KiB of identical bytes must gzip far below raw length")
        self.assertEqual(got, native_size.gzip9_bytes(payload))


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


class BudgetAndArtifact(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        self.binary = self.root / "vue-vet.bin"
        self.binary.write_bytes(b"\x7fELFfake")
        self.budget_path = self.root / "budget.json"

    def tearDown(self) -> None:
        self.tmp.cleanup()

    def write_budget(self, targets: dict) -> Path:
        self.budget_path.write_text(json.dumps({"schema_version": 1, "targets": targets}), encoding="utf-8")
        return self.budget_path

    def run_script(self, extra: list[str]) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [sys.executable, str(SCRIPT_DIR / "native_size.py"), *extra],
            capture_output=True,
            text=True,
        )

    def test_baseline_within_budget(self) -> None:
        measured = native_size.measure_binary(self.binary)
        self.write_budget(
            {
                "aarch64-apple-darwin": {
                    "file_bytes": measured["file_bytes"] + 100,
                    "gzip9_bytes": measured["gzip9_bytes"] + 100,
                }
            }
        )
        proc = self.run_script(
            [
                "--binary",
                str(self.binary),
                "--target",
                "aarch64-apple-darwin",
                "--budget-file",
                str(self.budget_path),
                "--json",
            ]
        )
        self.assertEqual(proc.returncode, 0, proc.stderr)
        payload = json.loads(proc.stdout)
        self.assertTrue(payload["ok"])
        self.assertEqual(payload["file_bytes"], measured["file_bytes"])

    def test_exact_limit_fits(self) -> None:
        measured = native_size.measure_binary(self.binary)
        self.write_budget(
            {
                "aarch64-apple-darwin": {
                    "file_bytes": measured["file_bytes"],
                    "gzip9_bytes": measured["gzip9_bytes"],
                }
            }
        )
        proc = self.run_script(
            [
                "--binary",
                str(self.binary),
                "--target",
                "aarch64-apple-darwin",
                "--budget-file",
                str(self.budget_path),
            ]
        )
        self.assertEqual(proc.returncode, 0, proc.stderr)

    def test_file_over_budget_fails(self) -> None:
        measured = native_size.measure_binary(self.binary)
        self.write_budget(
            {
                "aarch64-apple-darwin": {
                    "file_bytes": measured["file_bytes"] - 1,
                    "gzip9_bytes": measured["gzip9_bytes"] + 1000,
                }
            }
        )
        proc = self.run_script(
            [
                "--binary",
                str(self.binary),
                "--target",
                "aarch64-apple-darwin",
                "--budget-file",
                str(self.budget_path),
                "--json",
            ]
        )
        self.assertEqual(proc.returncode, native_size.EXIT_OVER_BUDGET)
        self.assertIn("file_bytes", proc.stderr)
        payload = json.loads(proc.stdout)
        self.assertFalse(payload["ok"])

    def test_gzip_over_budget_fails(self) -> None:
        measured = native_size.measure_binary(self.binary)
        self.write_budget(
            {
                "aarch64-apple-darwin": {
                    "file_bytes": measured["file_bytes"] + 1000,
                    "gzip9_bytes": measured["gzip9_bytes"] - 1,
                }
            }
        )
        proc = self.run_script(
            [
                "--binary",
                str(self.binary),
                "--target",
                "aarch64-apple-darwin",
                "--budget-file",
                str(self.budget_path),
            ]
        )
        self.assertEqual(proc.returncode, native_size.EXIT_OVER_BUDGET)
        self.assertIn("gzip9_bytes", proc.stderr)

    def test_unknown_target(self) -> None:
        self.write_budget({"aarch64-apple-darwin": {"file_bytes": 10, "gzip9_bytes": 10}})
        proc = self.run_script(
            [
                "--binary",
                str(self.binary),
                "--target",
                "wasm32-wasi",
                "--budget-file",
                str(self.budget_path),
            ]
        )
        self.assertEqual(proc.returncode, native_size.EXIT_OPERATIONAL)
        self.assertIn("unrecognized target", proc.stderr)

    def test_unknown_budget_entry(self) -> None:
        self.write_budget({"x86_64-unknown-linux-gnu": {"file_bytes": 10, "gzip9_bytes": 10}})
        proc = self.run_script(
            [
                "--binary",
                str(self.binary),
                "--target",
                "aarch64-apple-darwin",
                "--budget-file",
                str(self.budget_path),
            ]
        )
        self.assertEqual(proc.returncode, native_size.EXIT_OPERATIONAL)
        self.assertIn("unknown target", proc.stderr)

    def test_malformed_budget(self) -> None:
        self.budget_path.write_text("{not json", encoding="utf-8")
        proc = self.run_script(
            [
                "--binary",
                str(self.binary),
                "--target",
                "aarch64-apple-darwin",
                "--budget-file",
                str(self.budget_path),
            ]
        )
        self.assertEqual(proc.returncode, native_size.EXIT_OPERATIONAL)
        self.assertIn("not valid JSON", proc.stderr)

    def test_invalid_budget_values(self) -> None:
        self.write_budget({"aarch64-apple-darwin": {"file_bytes": 0, "gzip9_bytes": 12}})
        proc = self.run_script(
            [
                "--binary",
                str(self.binary),
                "--target",
                "aarch64-apple-darwin",
                "--budget-file",
                str(self.budget_path),
            ]
        )
        self.assertEqual(proc.returncode, native_size.EXIT_OPERATIONAL)
        self.assertIn("positive integer", proc.stderr)

    def test_missing_artifact(self) -> None:
        proc = self.run_script(
            ["--binary", str(self.root / "missing"), "--target", "aarch64-apple-darwin"]
        )
        self.assertEqual(proc.returncode, native_size.EXIT_OPERATIONAL)
        self.assertIn("not found", proc.stderr)

    def test_empty_artifact(self) -> None:
        empty = self.root / "empty.bin"
        empty.write_bytes(b"")
        proc = self.run_script(["--binary", str(empty), "--target", "aarch64-apple-darwin"])
        self.assertEqual(proc.returncode, native_size.EXIT_OPERATIONAL)
        self.assertIn("empty", proc.stderr)

    def test_binary_without_target_does_not_cargo(self) -> None:
        proc = self.run_script(["--binary", str(self.binary)])
        self.assertEqual(proc.returncode, native_size.EXIT_OPERATIONAL)
        self.assertIn("requires both --binary", proc.stderr)

    def test_committed_budget_has_positive_maxima_for_five_targets(self) -> None:
        budget = SCRIPT_DIR.parent / "fixtures" / "quality" / "native-size-budget.json"
        payload = json.loads(budget.read_text(encoding="utf-8"))
        for target in native_size.KNOWN_TARGETS:
            entry = payload["targets"][target]
            self.assertIsInstance(entry["file_bytes"], int)
            self.assertIsInstance(entry["gzip9_bytes"], int)
            self.assertGreater(entry["file_bytes"], 0)
            self.assertGreater(entry["gzip9_bytes"], 0)
            loaded = native_size.load_target_budget(budget, target)
            self.assertEqual(loaded, {"file_bytes": entry["file_bytes"], "gzip9_bytes": entry["gzip9_bytes"]})

    def test_maxima_are_ceil_103_of_measured_candidates(self) -> None:
        budget = SCRIPT_DIR.parent / "fixtures" / "quality" / "native-size-budget.json"
        payload = json.loads(budget.read_text(encoding="utf-8"))

        def ceil_103(n: int) -> int:
            return (n * 103 + 99) // 100

        for target in native_size.KNOWN_TARGETS:
            cand = payload["measured_candidate"]["targets"][target]
            base = payload["measured_baseline"]["targets"][target]
            maxima = payload["targets"][target]
            self.assertEqual(maxima["file_bytes"], ceil_103(cand["file_bytes"]))
            self.assertEqual(maxima["gzip9_bytes"], ceil_103(cand["gzip9_bytes"]))
            self.assertLessEqual(cand["file_bytes"], maxima["file_bytes"])
            self.assertLessEqual(cand["gzip9_bytes"], maxima["gzip9_bytes"])
            self.assertTrue(
                base["file_bytes"] > maxima["file_bytes"] or base["gzip9_bytes"] > maxima["gzip9_bytes"],
                target,
            )

    def test_invalid_utf8_budget_is_operational(self) -> None:
        self.budget_path.write_bytes(b"\xff\xfe{ not utf-8")
        proc = self.run_script(
            [
                "--binary",
                str(self.binary),
                "--target",
                "aarch64-apple-darwin",
                "--budget-file",
                str(self.budget_path),
            ]
        )
        self.assertEqual(proc.returncode, native_size.EXIT_OPERATIONAL)
        self.assertIn("UTF-8", proc.stderr)
        self.assertNotIn("Traceback", proc.stderr)

    def test_binary_budget_path_is_operational(self) -> None:
        ls_path = Path("/bin/ls")
        if not ls_path.is_file():
            self.skipTest("/bin/ls not present")
        proc = self.run_script(
            [
                "--binary",
                str(self.binary),
                "--target",
                "aarch64-apple-darwin",
                "--budget-file",
                str(ls_path),
            ]
        )
        self.assertEqual(proc.returncode, native_size.EXIT_OPERATIONAL)
        self.assertTrue("UTF-8" in proc.stderr or "not valid JSON" in proc.stderr)
        self.assertNotIn("Traceback", proc.stderr)

    def test_unreadable_artifact_is_operational(self) -> None:
        locked = self.root / "locked.bin"
        locked.write_bytes(b"\x7fELFfake")
        locked.chmod(0)
        try:
            proc = self.run_script(["--binary", str(locked), "--target", "aarch64-apple-darwin"])
            if proc.returncode == 0:
                self.skipTest("owner can still read mode-0 files on this host")
            self.assertEqual(proc.returncode, native_size.EXIT_OPERATIONAL)
            self.assertTrue("cannot read" in proc.stderr or "cannot stat" in proc.stderr)
            self.assertNotIn("Traceback", proc.stderr)
        finally:
            locked.chmod(0o644)


@unittest.skipIf(sys.platform == "win32", "PATH cargo stubs are covered on Linux CI")
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
            self.assertEqual(proc.returncode, native_size.EXIT_OPERATIONAL)
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
