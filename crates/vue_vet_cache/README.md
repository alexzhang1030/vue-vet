# vue_vet_cache

Content-addressed **scan cache**, **baselines**, and **git-diff filtering** for
[Vue Vet](https://github.com/alexzhang1030/vue-vet).

Caches only normalized `ScanSummary` and `ProjectGraph` — never Vize or Oxc AST
objects. Keys mix format / conventions / ruleset versions, tool versions,
serialized effective config, and every analyzed source path + body. Writes are
atomic (temp file + rename); corrupt or unsupported entries are deleted and
rebuilt without failing the scan.

## Status

Workspace-internal (`publish = false`). Consumed by `vue_vet_session` and the
CLI. Format constants: `CACHE_FORMAT_VERSION`, `BASELINE_FORMAT_VERSION`,
`RULESET_VERSION`.

## Surface

| Item | Role |
| --- | --- |
| `CacheStore` | Disk lookup / store for `CachePayload` |
| `content_key` | SHA-256 key over files + config bytes |
| `Baseline` | Fingerprinted finding set for `--baseline` |
| `read_git_diff` / `filter_diff` | `--diff <ref>` path/line filtering |

Fix modes bypass the cache so mutation always starts from a fresh scan.

## Related docs

- [Cache, baselines, and diff](../../docs/cache-baseline-diff.md)
- [architecture PCR](../../.agents/docs/architecture.md)
- [Workspace crates](../../docs/crates.md)

## License

MIT
