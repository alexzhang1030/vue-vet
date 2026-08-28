# vue_vet_cache

Content-addressed **scan cache**, **baselines**, and **git-diff filtering** for
[Vue Vet](https://github.com/alexzhang1030/vue-vet).

Caches only normalized `ScanSummary` + `ProjectGraph`. Never persists Vize or
Oxc AST. Does not run analysis, load config, or own session state — callers
(`vue_vet_session`, CLI) supply bytes and apply presentation filters.

## Status

Workspace-internal (`publish = false`).

## Format versions

| Constant | Value | Role |
| --- | --- | --- |
| `CACHE_FORMAT_VERSION` | `5` | On-disk entry schema; directory `v5/` |
| `BASELINE_FORMAT_VERSION` | `1` | Baseline file schema |
| `RULESET_VERSION` | `3` | Bump when built-in / seed-aware rule behavior changes |

`content_key` SHA-256 fields (sorted file path+body last):

- `cache-format`, `tool-version` (`CARGO_PKG_VERSION`)
- `vize-version`, `oxc-version` (string literals in this crate — bump them or
  `CACHE_FORMAT_VERSION` when a dependency upgrade changes results)
- `oxc-resolver-version` (`OXC_RESOLVER_VERSION` from `vue_vet_project`)
- `conventions-version`, `ruleset-version`, `reactivity-graph-version`
- serialized effective config bytes

## Public API

| Item | Role |
| --- | --- |
| `CacheStore::{new, entry_path, load, store}` | Disk lookup / atomic write |
| `CachePayload` | `{ summary, graph }` |
| `CacheLookup::{Hit, Miss, RecoveredCorruption}` | Load outcome |
| `content_key` | Deterministic key over files + config |
| `default_cache_dir` | `$XDG_CACHE_HOME/vue-vet` or temp `vue_vet_cache` |
| `Baseline::{from_summary, filter, read, write}` | Fingerprinted finding set |
| `diagnostic_fingerprint` | Rule + path + offset + message |
| `read_git_diff` / `ChangedLines` / `filter_diff` | `--diff <ref>` |

## Constraints

- Writes: temp file + rename. Invalid JSON / wrong version → delete entry and
  return `RecoveredCorruption` (scan continues).
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
