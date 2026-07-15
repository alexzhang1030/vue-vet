# Roadmap

## M0 — runnable vertical slice

- [x] Rust workspace and CLI
- [x] `.vue` discovery with ignore support
- [x] Vize SFC parsing
- [x] stable diagnostic model
- [x] text and JSON reporters
- [x] deterministic score and CI exit policy
- [x] first built-in diagnostic (`vue-vet/security/no-v-html`)
- [ ] validate on Linux, macOS, and Windows CI

## M1 — useful local doctor

- Vize template AST traversal instead of token-level rule matching
- Oxc semantic analysis for `<script>` and `<script setup>`
- 15 high-confidence Vue rules across correctness, reactivity, performance,
  accessibility, and maintainability
- TOML configuration, per-rule severity, inline suppression, and baselines
- snapshot fixtures against Vue compiler behavior

## M2 — project intelligence

- import and component graph for Vue, Nuxt, composables, and auto-imports
- unused component/composable detection
- cross-file prop, emit, slot, route, and store checks
- content-addressed cache and parallel scanning
- changed-file and changed-line modes

## M3 — extensibility and CI

- ast-grep-backed YAML rules for template/script structural patterns
- JSON Schema for rule configuration
- SARIF and GitHub annotations
- machine-readable fixes with safe autofix transactions
- published native binaries and npm launcher

## M4 — editor and agent surface

- LSP diagnostics and code actions
- explain mode with evidence and rule documentation
- MCP/agent interface over the same diagnostic engine
- benchmark and precision suite against representative Vue/Nuxt repositories

