# SARIF and GitHub annotations

Vue Vet supports two CI-oriented reporter formats in addition to text and JSON v1.

```sh
vue-vet . --format sarif > vue-vet.sarif
vue-vet . --format github
```

`--format sarif` emits SARIF 2.1.0. Results use repository-relative, slash-normalized artifact URIs, one-based source locations, stable Vue Vet diagnostic IDs under `partialFingerprints`, and rule help links to the repository documentation. The output is deterministic for a fixed scan result and can be uploaded with `github/codeql-action/upload-sarif`.

`--format github` emits GitHub Actions workflow commands. Effective severities map to `notice`, `warning`, and `error`; file, line, column, and rule ID are carried as annotation metadata. Percent signs, CR/LF characters, colons, and commas are escaped before output so diagnostics cannot inject additional workflow commands or corrupt command properties.

These reporters do not change scan or exit semantics. Warnings remain non-fatal unless `--deny-warnings` is supplied, errors remain fatal, and operational failures return exit code 2 on stderr.
