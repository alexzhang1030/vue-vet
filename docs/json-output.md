# JSON output contract

Vue Vet emits machine-readable results with `--format json`. The current wire
format is version 1:

```json
{
  "schema_version": 1,
  "tool": { "name": "vue-vet", "version": "0.1.0" },
  "ok": true,
  "mode": "full",
  "project": {
    "root": ".",
    "framework": "vue",
    "analyzed_files": ["src/App.vue"],
    "analyzed_file_count": 1,
    "files_scanned": 1,
    "complete": true,
    "skipped_checks": [],
    "skipped_check_reasons": {}
  },
  "diagnostics": [],
  "summary": {
    "score": 100,
    "finding_count": 0,
    "affected_file_count": 0,
    "by_severity": { "info": 0, "warning": 0, "error": 0 }
  },
  "reactivity": {
    "modules": 2,
    "bindings": 3,
    "scopes": 1,
    "edges": 2,
    "template_reads": 1,
    "hotspots": [
      {
        "id": "App.vue",
        "bindings": 2,
        "scopes": 1,
        "edges": 2,
        "template_reads": 1
      }
    ]
  },
  "error": null
}
```

`reactivity` is an additive optional field (still `schema_version` 1). It summarizes the
static tracer so consumers can tell a clean score is not the same as “tracer did
nothing.” Totals cover traced modules, bindings, tracking scopes, dependency
edges, and template joins. `hotspots` lists up to five busiest modules. When
tracing fails, `error` is set on this object (and `project.skipped_checks`
includes `module_reactivity`). `--print-reactivity` also fills
`modules_detail` with per-module binding/scope/edge/template **string labels**
plus structured span details for editor consumers:

```json
{
  "id": "App.vue",
  "bindings": ["error:ref"],
  "scopes": [],
  "edges": ["template:if@11768 -> error"],
  "template_reads": ["error@if"],
  "binding_details": [
    {
      "name": "error",
      "kind": "ref",
      "span": { "offset": 420, "length": 5 },
      "label": "error  (ref)"
    }
  ],
  "edge_details": [
    {
      "from": "template:if@11768",
      "to": "error",
      "to_id": "error@420",
      "kind": "template",
      "span": { "offset": 11768, "length": 5 },
      "to_span": { "offset": 420, "length": 5 },
      "to_path": "error",
      "label": "v-if  →  error"
    },
    {
      "from": "label",
      "to": "props",
      "to_id": "props@420",
      "property": "count",
      "to_path": "props.count",
      "kind": "computed",
      "span": { "offset": 80, "length": 5 },
      "to_span": { "offset": 420, "length": 5 },
      "label": "label  →  props.count"
    }
  ],
  "scope_details": [
    {
      "kind": "computed",
      "callee": "computed",
      "binding": "derived",
      "span": { "offset": 80, "length": 40 },
      "label": "computed  →  derived",
      "uncertain_accesses": ["mystery"]
    }
  ],
  "template_details": [
    {
      "binding": "error",
      "surface": "if",
      "span": { "offset": 11768, "length": 5 },
      "label": "v-if  reads  error"
    }
  ]
}
```

`scope_details[].uncertain_accesses` lists soft under-approx evidence: identifier
roots of `.value` / `unref` / `toValue` (or bare watch sources) that were analyzed
but not classified as known bindings. Absence rules report these as
`(maybe: …)` instead of inventing edges. The field is omitted when empty.
String `scopes` labels append ` maybe:a,b` when soft evidence is present.

`span` / `to_span` are source **byte** ranges (`offset` + `length`). Editors should
map them with UTF-8-aware `positionAt`. String label arrays remain for text
reports and older consumers; prefer `*_details` when present. Humanized `label`
fields match the reactivity TUI wording. When a dependency is a member read
(`props.count`), `property` and `to_path` are set; bare `to` stays the binding
name for rule matching.

JSON reports also include additive `component_nav` — a per-file `uses` /
`used_by` index of project-graph `component_usage` and `auto_component` edges
(template tag evidence spans). This is **structural component navigation**, not
parent `:prop` → child `defineProps` dataflow:

```json
{
  "modules": [
    {
      "id": "pages/index.vue",
      "uses": [
        {
          "peer": "components/Demo.vue",
          "kind": "auto_component",
          "specifier": "Demo",
          "span": { "offset": 40, "length": 4 }
        }
      ],
      "used_by": []
    }
  ]
}
```

Default `--format text` prints the same digest under a `Reactivity` footer after
the score line. The thin VS Code host under `editors/vscode/` consumes
`--format json --print-reactivity` and does not re-implement the tracer.

## Diagnostic identity

Each diagnostic includes an opaque `id` with a readable prefix:

```text
<normalized-file>::<line>:<column>::<rule-id>::<content-digest>
```

The identity is deterministic for an unchanged finding. It changes when its
normalized location, rule, severity, or user-visible message changes. Consumers
must compare it as an opaque string rather than parsing or constructing it.

Diagnostic `file` values and `project.analyzed_files` are relative to
`project.root` and use `/` separators on every operating system. `confidence` and `documentation`
come from Vue Vet-owned rule metadata. `documentation` is a repository-local
Markdown path so local tools and coding agents can read the exact rule guidance
without a network request.

An active diagnostic may include an optional `edits` array. Each edit has a
normalized repository-relative `file`, original-source byte `range`, exact
`replacement`, `applicability`, and originating `rule_id`. Absence of `edits`
means Vue Vet has no machine-authorized change for that finding; consumers must
not synthesize one from the diagnostic span. `--fix-dry-run` is the supported
way to validate and inspect the current safe plan without writing files.

Practice suggestions (`category: "practice"`) may also include an additive
`recommendation` object (still `schema_version` 1). They appear in
`diagnostics` and text `Suggestions` sections, but do **not** reduce
`summary.score` or fail the default CI exit. Set `practice = "off"` in
`vue-vet.toml` to drop the channel before reporting. Example:

```json
{
  "rule_id": "vue-vet/practice/vueuse-use-debounce-fn",
  "category": "practice",
  "severity": "info",
  "recommendation": {
    "kind": "ecosystem_api",
    "package": "@vueuse/core",
    "export": "useDebounceFn",
    "docs_url": "https://vueuse.org/core/useDebounceFn/",
    "import_example": "import { useDebounceFn } from '@vueuse/core'"
  }
}
```

LSP diagnostic `data` is a JSON object with opaque finding `id` and the same
optional `recommendation` when present. Finding `--explain` JSON nests
`recommendation` beside the rule docs payload.

```json
{
  "file": "src/App.vue",
  "range": { "offset": 42, "length": 10 },
  "replacement": "",
  "applicability": "safe",
  "rule_id": "vue-vet/accessibility/no-autofocus"
}
```

## Completeness

An empty `diagnostics` array is clean only when `project.complete` is `true`.
Consumers must inspect `skipped_checks` and `skipped_check_reasons` when it is
false. `analyzed_files` is sorted and deduplicated so CI and agent consumers can
verify exact coverage instead of inferring it from a count.

`mode` is one of `full`, `baseline`, or `diff`. Filtering changes the reported
findings, not the analyzed-file coverage.

Recoverable analysis failures produce a partial result rather than an
operational failure. For example, an unfinished Vue file keeps `ok: true`, sets
`project.complete` to `false`, records the skipped project checks, and emits a
`vue-vet/analysis/parse-error` diagnostic for that file while continuing to
analyze the rest of the workspace. A partial result follows the normal finding
exit-code policy (typically exit code 1 when the parser diagnostic is active).

With `--format json`, operational failures also use version 1 and retain exit
code 2. They set `ok` and `project.complete` to `false`, leave diagnostics and
coverage empty when the scan never completed, set `summary.score` to `null`, and
provide the actionable failure in `error.message`. Text mode continues to write
operational failures to stderr. Invalid configuration, an unreadable root, and
other failures that prevent workspace analysis from starting remain operational
failures.

## `--explain` (rule or finding documentation)

`--explain <RULE_OR_FINDING>` exits after printing documentation. Lookup and
scan orchestration live in `vue_vet_session` (shared with LSP/MCP);
the CLI only formats stdout. With `--format json` it prints a standalone
object — not wrapped in `schema_version` / `diagnostics`.

### Rule id

A full rule id (for example `vue-vet/security/no-v-html`) is an early-exit lookup
with no scan:

```json
{
  "rule_id": "vue-vet/security/no-v-html",
  "category": "security",
  "severity": "warning",
  "confidence": "high",
  "documentation": "docs/rules/security/no-v-html.md",
  "body": "# `vue-vet/security/no-v-html`\n…",
  "body_path": "/path/to/docs/rules/security/no-v-html.md"
}
```

`documentation` matches the path shape on scan diagnostics. When the Markdown
file cannot be found (for example a binary-only install without the docs tree),
`body` is omitted and `body_error` explains why. Unknown rule ids use the normal
operational-failure contract (exit 2). Text mode prints the same fields as a
short header plus the Markdown body.

### Finding id

An opaque diagnostic id from a prior JSON report (values containing `::`) triggers
a scan of the CLI path, exact id match, then evidence plus nested rule docs:

```json
{
  "id": "basic.vue::2:9::vue-vet/security/no-v-html::…",
  "file": "basic.vue",
  "span": { "offset": 19, "length": 6, "line": 2, "column": 9 },
  "severity": "warning",
  "confidence": "high",
  "message": "`v-html` can render untrusted HTML into the page",
  "help": "Prefer normal template interpolation. …",
  "rule": {
    "rule_id": "vue-vet/security/no-v-html",
    "category": "security",
    "severity": "warning",
    "confidence": "high",
    "documentation": "docs/rules/security/no-v-html.md",
    "body": "# `vue-vet/security/no-v-html`\n…"
  }
}
```

Re-run with the same scan path that produced the id. A missing match is an
operational failure (exit 2). Consumers still treat `id` as opaque; the CLI only
matches the full string.

When the finding’s span sits inside a tracking scope (computed, `watchEffect`,
…), JSON may include optional `tracking` — the same payload as
[`--explain-scope`](#explain-scope-tracking-scope) for that scope. Text mode
appends the scope section after rule docs.

## `--explain-scope` (tracking scope)

`--explain-scope <QUERY>` scans the CLI path and answers **would Vue re-run this
scope when state changes?** from static tracer facts (not DevTools). With
`--format json` it prints a standalone `ScopeExplain` object (or an array when
multiple scopes match) — not wrapped in `schema_version` / `diagnostics`. The
MCP tool `vue_vet_explain_scope` returns the same JSON (`query` plus optional
scan `path`).

### Query forms

| Query | Matches |
| --- | --- |
| `label` | Scope binding `label` (or callee name) |
| `App.vue:label` | Binding in modules whose id ends with `App.vue` |
| `App.vue:` / `App.vue` | All scopes in matching modules (path-like / extension) |
| `@42` | Scope whose span starts at byte offset 42 |
| `computed@42` | Callee (or binding / module) + span offset |

Unknown queries are operational failures (exit 2).

### JSON shape

```json
{
  "module_id": "placeholder.vue",
  "kind": "computed",
  "callee": "computed",
  "binding": "label",
  "span": { "offset": 48, "length": 24, "line": 3, "column": 15 },
  "summary": "`label` has no known reactive dependency — Vue will not re-run it when state changes",
  "tracks": [],
  "does_not_track": [],
  "uncertain": []
}
```

- `tracks` — dependency paths that participate in tracking (`unconditional` /
  `conditional`), each with `path`, `reason`, `reason_label`, `span`, optional
  `guards`.
- `does_not_track` — reads that do **not** establish tracking (`after_await`,
  `outside_tracking`).
- `uncertain` — soft `maybe:` roots that were not classified as known bindings.
- `summary` — one-line human/agent verdict.

Text mode prints the same fields as a short header (`tracking scope`, module,
kind, summary, tracks / does not track lists).

## Agent consumption

The JSON report is the complete fact layer, not a generated fix prompt. Agents
should group diagnostics by `rule_id`, prioritize severity and confidence, read
the referenced source and local documentation (`--explain` / MCP `vue_vet_explain`
or the `documentation` path), and ask `--explain-scope` / MCP
`vue_vet_explain_scope` when the question is “would Vue re-run this scope?”.
Verify a finding before editing. An edit is actionable only when Vue
Vet emits it with explicit applicability; a diagnostic without one remains
manual. Future bounded handoff prompts may point to this report, but must not
replace it or silently omit lower-priority findings.

## Compatibility

- `schema_version` is required; consumers must branch on versions they support.
- Version 1 is the initial public report contract and includes explicit tool,
  mode, coverage, identity, metadata, summary, and error fields.
- Before the first alpha release, version 1 may change without compatibility
  guarantees. After alpha, incompatible changes require a new schema version.
- Adding optional fields is backward-compatible within version 1.
- Removing fields, changing their meaning or type, or changing diagnostic
  identity semantics requires a new schema version.
- Object field order is not significant. Array order is deterministic.

Configuration, graph debug output, cache files, and baselines remain
independently versioned formats.
