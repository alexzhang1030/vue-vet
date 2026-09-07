#!/usr/bin/env python3
"""Check-only inventory for crates/vue_vet_rules/src/rules/matrix/mod.rs.

This script does **not** write Rust. Matrix behavior is hand-maintained in
`matrix/mod.rs`. A previous generator used a stale implementation template and
would overwrite live rule logic.

Usage:
  python3 scripts/gen_matrix_rules.py
Exit 0 when live IDs match and retired IDs are absent.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MATRIX = ROOT / "crates/vue_vet_rules/src/rules/matrix/mod.rs"

# Removed from the product catalog. Must stay absent from matrix/mod.rs.
RETIRED_IDS = {
  "vue-vet/correctness/no-define-emits-after-await",
  "vue-vet/correctness/no-define-model-after-await",
  "vue-vet/correctness/no-define-options-after-await",
  "vue-vet/correctness/no-define-props-after-await",
  "vue-vet/correctness/no-define-slots-after-await",
  "vue-vet/correctness/no-effect-scope-after-await",
  "vue-vet/correctness/no-get-current-instance-after-await",
  "vue-vet/correctness/no-inject-after-await",
  "vue-vet/correctness/no-next-tick-after-await",
  "vue-vet/correctness/no-on-activated-after-await",
  "vue-vet/correctness/no-on-before-mount-after-await",
  "vue-vet/correctness/no-on-before-unmount-after-await",
  "vue-vet/correctness/no-on-before-update-after-await",
  "vue-vet/correctness/no-on-deactivated-after-await",
  "vue-vet/correctness/no-on-error-captured-after-await",
  "vue-vet/correctness/no-on-mounted-after-await",
  "vue-vet/correctness/no-on-render-tracked-after-await",
  "vue-vet/correctness/no-on-render-triggered-after-await",
  "vue-vet/correctness/no-on-server-prefetch-after-await",
  "vue-vet/correctness/no-on-unmounted-after-await",
  "vue-vet/correctness/no-on-updated-after-await",
  "vue-vet/correctness/no-provide-after-await",
  "vue-vet/correctness/no-use-attrs-after-await",
  "vue-vet/correctness/no-use-css-module-after-await",
  "vue-vet/correctness/no-use-css-vars-after-await",
  "vue-vet/correctness/no-use-slots-after-await",
  "vue-vet/correctness/no-watch-after-await",
  "vue-vet/correctness/no-watch-effect-after-await",
  "vue-vet/correctness/no-watch-post-effect-after-await",
  "vue-vet/correctness/no-watch-sync-effect-after-await",
  "vue-vet/correctness/no-with-defaults-after-await",
  "vue-vet/reactivity/no-conditional-dependency-in-computed",
  "vue-vet/reactivity/no-conditional-dependency-in-effect-scope",
  "vue-vet/reactivity/no-conditional-dependency-in-render",
  "vue-vet/reactivity/no-conditional-dependency-in-watch-sources",
  "vue-vet/reactivity/no-conditional-watch-effect-dependency",
  "vue-vet/reactivity/prefer-explicit-sources-for-conditional-deps",
  "vue-vet/reactivity/no-self-trigger-in-watch-effect",
  "vue-vet/reactivity/no-self-trigger-in-watch-post-effect",
  "vue-vet/reactivity/no-self-trigger-in-watch-sync-effect",
}

# IDs this file is expected to own. Other builtins live outside matrix/.
LIVE_MATRIX_IDS = {
  "vue-vet/reactivity/no-after-await-dependency-in-computed",
  "vue-vet/reactivity/no-outside-tracking-dependency-in-computed",
  "vue-vet/reactivity/no-after-await-dependency-in-watch-sources",
  "vue-vet/reactivity/no-outside-tracking-dependency-in-watch-sources",
  "vue-vet/reactivity/no-after-await-dependency-in-effect-scope",
  "vue-vet/reactivity/no-outside-tracking-dependency-in-effect-scope",
  "vue-vet/correctness/no-define-expose-after-await",
  "vue-vet/reactivity/no-computed-self-trigger",
  "vue-vet/reactivity/no-side-effects-in-computed",
  "vue-vet/reactivity/no-effect-write-without-read",
  "vue-vet/reactivity/no-computed-without-dependency",
  "vue-vet/reactivity/prefer-watch-over-effect-for-single-source",
  "vue-vet/reactivity/no-assignment-only-effect-with-conditional-read",
  "vue-vet/reactivity/no-on-scope-dispose-reactive-read",
  "vue-vet/reactivity/no-empty-watch-sources",
  "vue-vet/reactivity/no-watch-callback-as-tracking-scope",
  "vue-vet/reactivity/no-reactive-destructure",
  "vue-vet/reactivity/no-shallow-reactive-destructure",
  "vue-vet/reactivity/prefer-store-to-refs",
  "vue-vet/reactivity/no-route-destructure",
  "vue-vet/reactivity/no-router-destructure",
  "vue-vet/reactivity/no-ref-as-operand",
  "vue-vet/reactivity/no-model-ref-as-operand",
  "vue-vet/reactivity/no-computed-as-operand",
  "vue-vet/reactivity/no-readonly-mutation",
}

ID_RE = re.compile(r'"(vue-vet/[^"]+)"')


def main() -> int:
  if "--write" in sys.argv:
    print(
      "refusing --write: matrix/mod.rs is hand-maintained; this script is check-only",
      file=sys.stderr,
    )
    return 2
  text = MATRIX.read_text(encoding="utf-8")
  found = set(ID_RE.findall(text))
  restored = sorted(RETIRED_IDS & found)
  missing = sorted(LIVE_MATRIX_IDS - found)
  extra = sorted(found - LIVE_MATRIX_IDS)
  errors: list[str] = []
  if restored:
    errors.append(f"retired IDs present in matrix/mod.rs: {restored}")
  if missing:
    errors.append(f"expected live matrix IDs missing: {missing}")
  if extra:
    errors.append(f"unexpected IDs in matrix/mod.rs (update LIVE_MATRIX_IDS if intentional): {extra}")
  if "TemplateRefSetupReadRule" in text or "I_TEMPLATE_REF_SETUP_READ" in text:
    errors.append("dead TemplateRefSetupRead reservation is still in matrix/mod.rs")
  if errors:
    print("\n".join(errors), file=sys.stderr)
    return 1
  print(f"ok: {MATRIX.relative_to(ROOT)} owns {len(found)} live IDs; {len(RETIRED_IDS)} retired IDs absent")
  return 0


if __name__ == "__main__":
  raise SystemExit(main())
