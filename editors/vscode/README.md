# Vue Vet Reactivity (VS Code)

Thin editor host for [Vue Vet](https://github.com/alexzhang1030/vue-vet) static
reactivity tracing. The extension **does not** re-implement analysis: it runs
the native CLI and paints the returned spans.

```text
Command Palette → Vue Vet: Show Reactivity
  → vue-vet <workspace> --format json --print-reactivity
    → TreeView + decorations + hover
```

This is **not** the M4 LSP surface tracked in issue #12. Diagnostics, code
actions, and incremental document sync stay out of scope here.

## Prerequisites

- VS Code 1.85+
- A Vue Vet binary resolved in this order:
  1. `vue-vet.path` setting (absolute path), or
  2. workspace `target/debug/vue-vet` or `target/release/vue-vet` (Cargo builds), or
  3. `vue-vet` on `PATH`, or
  4. fallback `npx --yes @vue-vet/cli`

When the CLI cannot start, the error toast offers **Open Settings** (jump to
`vue-vet.path`) and **Retry**.

For local development against this repo, either build the CLI (`cargo build -p vue-vet`)
so auto-detect finds `target/debug/vue-vet`, or set the path explicitly:

```json
{
  "vue-vet.path": "/absolute/path/to/vue-vet/target/debug/vue-vet"
}
```

## Install (from source)

```bash
cd editors/vscode
npm install
npm test
npm run package          # writes vue-vet-*.vsix
code --install-extension vue-vet-*.vsix
# or, without packaging: code --install-extension .
```

From a development host, open this folder in VS Code and use **Run Extension**
(`F5`) if you prefer an Extension Development Host. With the monorepo root as
the workspace and a built `target/debug/vue-vet`, no `vue-vet.path` is required.

## Commands

| Command | Action |
| --- | --- |
| `Vue Vet: Show Reactivity` | Scan workspace, fill the sidebar, decorate the active file |
| `Vue Vet: Refresh Reactivity` | Re-run the scan |
| `Vue Vet: Clear Reactivity Highlights` | Drop decorations and tree data |
| `Vue Vet: Show Who Reads This` | Right-click a binding (or `props.count`) → inbound readers |
| `Vue Vet: Show Dependencies` | Right-click a computed/effect binding → outbound deps |
| `Vue Vet: Show Components Used` | Structural uses from the project graph (not prop dataflow) |
| `Vue Vet: Show Component Users` | Structural used-by from the project graph (not prop dataflow) |
| `Vue Vet: Explain Scope (would Vue re-run?)` | Cursor byte offset → CLI `--explain-scope @offset` (covering fallback) |

## Settings

| Setting | Default | Meaning |
| --- | --- | --- |
| `vue-vet.path` | `""` | Absolute CLI path; empty uses workspace Cargo binary, then PATH, then npx |
| `vue-vet.refreshOnSave` | `false` | Re-trace on Vue/JS/TS save (off by default for large repos) |

## Tests

```bash
cd editors/vscode && npm test
```

Pure Node tests cover JSON → tree / decoration / hover planning and CLI
launcher resolution. No VS Code API is required for those checks.

## Span mapping

Vue Vet reports **UTF-8 byte** offsets. VS Code `positionAt` / `offsetAt` use
**UTF-16** code units. The extension converts before decorating or resolving
hovers — never pass byte offsets straight into `positionAt` (Unicode prefixes
would shift highlights to the right).
