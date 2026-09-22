# vue_vet_cache

Content-addressed **scan cache**, **baselines**, and **git-diff filtering** for
[Vue Vet](https://github.com/alexzhang1030/vue-vet).

Caches only normalized `ScanSummary` + `ProjectGraph`. Never persists Vize or
Oxc AST. Does not run analysis, load config, or own session state — callers
(`vue_vet_session`, CLI) supply bytes and apply presentation filters.

## Status

Workspace-internal (`publish = false`).

## Format versions

Values live in `src/lib.rs` (single source of truth; do not copy numbers here):

| Constant | Role |
| --- | --- |
| `CACHE_FORMAT_VERSION` | On-disk entry schema; directory `v<N>/` |
| `BASELINE_FORMAT_VERSION` | Baseline file schema |
| `RULESET_VERSION` | Bumped when the rule set / default channel behavior changes; doc comment carries the current reason |
| `CACHE_VIZE_CROQUIS_VERSION` | Hashed `vize-version` (`AnalysisStackIdentity`) |
| `CACHE_OXC_PARSER_VERSION` | Hashed `oxc-version` (`AnalysisStackIdentity`) |

`content_key` SHA-256 fields (sorted file path+body last):

- `cache-format`, `tool-version` (`CARGO_PKG_VERSION`)
- `vize-version`, `oxc-version`, `oxc-resolver-version` from
  `AnalysisStackIdentity::current()` (`CACHE_VIZE_CROQUIS_VERSION`,
  `CACHE_OXC_PARSER_VERSION`, `OXC_RESOLVER_VERSION`)
- `conventions-version`, `project-graph-schema-version`, `ruleset-version`, `reactivity-graph-version`
- serialized effective config bytes

`just compat-matrix` (`crates/vue_vet_cli/tests/compat_matrix.rs`) asserts those
identity constants against `fixtures/quality/compat-matrix.json`, the workspace
Cargo.toml pin, and Cargo.lock. Changing the hashed identity must change
`content_key` (`content_key_with_identity`); an `assert_ne!` on a stale string
alone does not prove the field participates in hashing.

## Public API

| Item | Role |
| --- | --- |
| `CacheStore::{new, entry_path, load, store}` | Disk lookup / atomic write |
| `CachePayload` | `{ summary, graph }` |
| `CacheLookup::{Hit, Miss, RecoveredCorruption, IncompatibleGraph}` | Load outcome |
| `CacheLookup::rejection` / `CacheRejection` | Stable reason for a lookup that reused no persisted work |
| `content_key` | Deterministic key over files + config + graph/rules/stack identities |
| `content_key_with_identity` | Same hash with an explicit identity (upgrade / miss tests) |
| `AnalysisStackIdentity` | `vize_croquis` / `oxc_parser` / `oxc_resolver` actually hashed |
| `default_cache_dir` | `$XDG_CACHE_HOME/vue-vet` or temp `vue_vet_cache` |
| `Baseline::{from_summary, filter, read, write}` | Fingerprinted finding set |
| `diagnostic_fingerprint` | Rule + path + offset + message |
| `read_git_diff` / `ChangedLines` / `filter_diff` | `--diff <ref>` |

## Constraints

- Writes: temp file + rename. Invalid JSON / wrong envelope or graph version →
  delete the entry and return the matching structured rejection (scan continues).
- Baseline and diff filtering happen **after** cache lookup so presentation
  modes do not fragment keys.
- `filter_diff` always retains `category == "project"` findings (remote cause,
  local location).
- Fix modes set `no_cache` so mutation always starts from a fresh scan.
- Paths in keys / fingerprints use `/`-normalized form.

## Related docs

- [Cache, baselines, and diff](../../docs/cache-baseline-diff.md) (product
  behavior; keep in sync with the constants above)
- [architecture PCR](../../.agents/docs/architecture.md) (identity / determinism)
- [Workspace crates](../../docs/crates.md)

## License

MIT
