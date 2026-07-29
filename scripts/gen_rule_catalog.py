#!/usr/bin/env python3
"""Generate docs/rules/README.md from RuleMeta documentation keys in source."""

from __future__ import annotations

import re
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "docs" / "rules" / "README.md"

ID_RE = re.compile(r'id:\s*"(vue-vet/(?P<cat>[^/"]+)/(?P<name>[^"]+))"')
DOC_RE = re.compile(r'documentation:\s*"(?P<doc>rules/[^"]+)"')
# Matrix / macro pairs: "vue-vet/cat/name", "rules/cat/name"
PAIR_RE = re.compile(
  r'"(vue-vet/(?P<cat>[^/"]+)/(?P<name>[^"]+))"\s*,\s*"(?P<doc>rules/[^"]+)"'
)

SCAN_DIRS = [
  ROOT / "crates" / "vue_vet_rules" / "src",
  ROOT / "crates" / "vue_vet_practice" / "src",
  ROOT / "crates" / "vue_vet_project" / "src",
]


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


def render(rules: dict[str, str]) -> str:
  by_cat: dict[str, list[tuple[str, str]]] = defaultdict(list)
  for rid, doc in sorted(rules.items()):
    cat = rid.split("/")[1]
    by_cat[cat].append((rid, doc))

  lines = [
    "# Rule catalog",
    "",
    "Generated from `RuleMeta` documentation keys. Regenerate with",
    "`python3 scripts/gen_rule_catalog.py`.",
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
    for rid, doc in by_cat[cat]:
      rel = "./" + doc.removeprefix("rules/") + ".md"
      path = ROOT / "docs" / f"{doc}.md"
      marker = "" if path.is_file() else " *(missing doc)*"
      lines.append(f"- [`{rid}`]({rel}){marker}")
    lines.append("")

  lines.extend(
    [
      "## How to read a rule",
      "",
      "- CLI: `vue-vet --explain <rule-id>`",
      "- Fixtures: `fixtures/rules/<name>/{invalid,valid}/`",
      "- Practice suggestions (`category: practice`) do not affect score by default",
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
