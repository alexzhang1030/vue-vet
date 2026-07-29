#!/usr/bin/env python3
"""Generate docs/rules/README.md from RuleMeta documentation keys in source."""

from __future__ import annotations

import re
from collections import Counter, defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "docs" / "rules" / "README.md"

ID_RE = re.compile(r'id:\s*"(vue-vet/(?P<cat>[^/"]+)/(?P<name>[^"]+))"')
DOC_RE = re.compile(r'documentation:\s*"(?P<doc>rules/[^"]+)"')
PAIR_RE = re.compile(
  r'"(vue-vet/(?P<cat>[^/"]+)/(?P<name>[^"]+))"\s*,\s*"(?P<doc>rules/[^"]+)"'
)

SCAN_DIRS = [
  ROOT / "crates" / "vue_vet_rules" / "src",
  ROOT / "crates" / "vue_vet_practice" / "src",
  ROOT / "crates" / "vue_vet_project" / "src",
]

# Stable overrides when heuristics would mis-label.
TRACER_FORCE = {
  "vue-vet/reactivity/prefer-computed",
  "vue-vet/reactivity/no-conditional-watch-effect-dependency",
  "vue-vet/reactivity/no-after-await-watch-effect-dependency",
  "vue-vet/reactivity/no-unused-reactive-binding",
  "vue-vet/reactivity/no-stale-prop-flow",
  "vue-vet/reactivity/no-nonreactive-props-destructure",
  "vue-vet/correctness/no-mutating-props",
}

PRACTICE_FORCE = {
  "vue-vet/reactivity/prefer-use-template-ref",
}

PARITY_FORCE = {
  "vue-vet/security/no-v-html",
  "vue-vet/maintainability/no-redundant-role",
}


def collect() -> dict[str, str]:
  rules: dict[str, str] = {}
  for directory in SCAN_DIRS:
    for path in directory.rglob("*.rs"):
      text = path.read_text()
      for match in PAIR_RE.finditer(text):
        rid = f"vue-vet/{match.group('cat')}/{match.group('name')}"
        rules[rid] = match.group("doc")
      for id_match in ID_RE.finditer(text):
        window = text[id_match.start() : id_match.start() + 500]
        doc_match = DOC_RE.search(window)
        if doc_match is None:
          continue
        rules[id_match.group(0).split('"')[1]] = doc_match.group("doc")
  return rules


def tier_for(rid: str) -> str:
  if rid in TRACER_FORCE:
    return "tracer"
  if rid in PRACTICE_FORCE:
    return "practice"
  if rid in PARITY_FORCE:
    return "parity"
  cat = rid.split("/")[1]
  name = rid.rsplit("/", 1)[-1]
  if cat == "practice" or rid.startswith("vue-vet/practice/"):
    return "practice"
  if cat == "accessibility":
    return "parity"
  if cat == "correctness":
    if name.endswith("-after-await"):
      return "parity"
    if name.startswith("valid-") or name.startswith("require-"):
      return "parity"
    if "deprecated" in name or "duplicate" in name:
      return "parity"
    if name in {
      "no-child-content",
      "no-template-key",
      "no-textarea-mustache",
      "no-v-text-v-html-on-component",
      "no-dupe-v-else-if",
      "no-duplicate-attributes",
      "no-import-compiler-macros",
      "no-v-if-with-v-for",
    }:
      return "parity"
    return "parity"
  if cat == "reactivity":
    return "tracer"
  if cat in {"security", "maintainability", "project"}:
    return "parity"
  return "parity"


def render(rules: dict[str, str]) -> str:
  by_cat: dict[str, list[tuple[str, str, str]]] = defaultdict(list)
  tiers = Counter()
  for rid, doc in sorted(rules.items()):
    tier = tier_for(rid)
    tiers[tier] += 1
    by_cat[rid.split("/")[1]].append((rid, doc, tier))

  lines = [
    "# Rule catalog",
    "",
    "Generated from `RuleMeta` documentation keys. Regenerate with",
    "`python3 scripts/gen_rule_catalog.py` (`just rules-catalog`).",
    "",
    "## Differentiation tiers",
    "",
    "Vue Vet ships a large built-in set. **Differentiation is the reactivity tracer**,",
    "not Essential/a11y parity with `eslint-plugin-vue`.",
    "",
    "| Tier | Meaning | Count |",
    "| --- | --- | ---: |",
    f"| `tracer` | Needs `vue_vet_reactivity` graph facts (read kinds, guards, scopes, prop edges, binding kinds) | {tiers['tracer']} |",
    f"| `parity` | Template Essential / a11y / macros / after-await registrars — open-box completeness | {tiers['parity']} |",
    f"| `practice` | Ecosystem suggestions (`category: practice`); excluded from score by default | {tiers['practice']} |",
    "",
    f"Total registered rules: **{len(rules)}**.",
    "",
    "| Category | Count |",
    "| --- | ---: |",
  ]
  for cat in sorted(by_cat):
    lines.append(f"| `{cat}` | {len(by_cat[cat])} |")
  lines.extend(["", "Per-rule pages live under `docs/rules/<category>/<name>.md`.", ""])

  for cat in sorted(by_cat):
    lines.extend([f"## {cat}", ""])
    for rid, doc, tier in by_cat[cat]:
      rel = "./" + doc.removeprefix("rules/") + ".md"
      path = ROOT / "docs" / f"{doc}.md"
      marker = "" if path.is_file() else " *(missing doc)*"
      lines.append(f"- [`{rid}`]({rel}) `{tier}`{marker}")
    lines.append("")

  lines.extend(
    [
      "## How to read a rule",
      "",
      "- CLI: `vue-vet --explain <rule-id>`",
      "- Fixtures: `fixtures/rules/<name>/{invalid,valid}/`",
      "- Practice suggestions do not affect score by default",
      "- Prefer tracer-tier findings when evaluating Vue Vet against other doctors",
      "",
    ]
  )
  return "\n".join(lines)


def main() -> None:
  rules = collect()
  if len(rules) < 80:
    raise SystemExit(f"expected a large catalog, got {len(rules)}")
  OUT.write_text(render(rules))
  print(f"wrote {OUT.relative_to(ROOT)} ({len(rules)} rules)")


if __name__ == "__main__":
  main()
