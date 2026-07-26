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
- A Vue Vet binary available as:
  1. `vue-vet.path` setting (absolute path), or
  2. `vue-vet` on `PATH`, or
  3. fallback `npx --yes @vue-vet/cli`

For local development against this repo, point the setting at the cargo binary:

```json
{
  "vue-vet.path": "/absolute/path/to/vue-vet/target/debug/vue-vet"
}
```

## Install (local)

```bash
cd editors/vscode
code --install-extension .   # or: vsce package && code --install-extension vue-vet-*.vsix
```

From a development host, open this folder in VS Code and use **Run Extension**
(`F5`) if you prefer an Extension Development Host.

## Commands

| Command | Action |
| --- | --- |
| `Vue Vet: Show Reactivity` | Scan workspace, fill the sidebar, decorate the active file |
| `Vue Vet: Refresh Reactivity` | Re-run the scan |
| `Vue Vet: Clear Reactivity Highlights` | Drop decorations and tree data |
| `Vue Vet: Show Who Reads This` | Right-click a binding (editor or tree) → inbound readers |
| `Vue Vet: Show Dependencies` | Right-click a computed/effect binding → outbound deps |

## Settings

| Setting | Default | Meaning |
| --- | --- | --- |
| `vue-vet.path` | `""` | Absolute CLI path |
| `vue-vet.refreshOnSave` | `false` | Re-trace on Vue/JS/TS save (off by default for large repos) |

## Tests

```bash
cd editors/vscode && npm test
```

Pure Node tests cover JSON → tree / decoration / hover planning. No VS Code
API is required for those checks.

## Span mapping

Vue Vet reports **UTF-8 byte** offsets. VS Code `positionAt` / `offsetAt` use
**UTF-16** code units. The extension converts before decorating or resolving
hovers — never pass byte offsets straight into `positionAt` (Unicode prefixes
would shift highlights to the right).
