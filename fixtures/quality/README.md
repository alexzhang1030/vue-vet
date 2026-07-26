# Quality corpus

Committed reference inputs for issue
[#13](https://github.com/alexzhang1030/vue-vet/issues/13).

- [`manifest.json`](./manifest.json) — corpus members and `tree_digest` values
- [`precision/`](./precision/) — labeled project finding expectations

Methodology and release checklists:
[docs/quality-gates.md](../../docs/quality-gates.md).

After editing a corpus project, run `just quality-digest` and update the digest
in `manifest.json` in the same change.
