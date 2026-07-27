# Quality corpus

Committed reference inputs for issue
[#13](https://github.com/alexzhang1030/vue-vet/issues/13).

- [`manifest.json`](./manifest.json) — corpus members and `tree_digest` values
- [`precision/`](./precision/) — labeled project finding expectations
- [`compat-matrix.json`](./compat-matrix.json) — Rust / Vize / Oxc / Vue pins

Methodology and release checklists:
[docs/quality-gates.md](../../docs/quality-gates.md),
[docs/quality-baselines.md](../../docs/quality-baselines.md).

After editing a corpus project, run `just quality-digest` and update the digest
in `manifest.json` in the same change.

## Project index

| Id | Path | Roles | Precision focus |
| --- | --- | --- | --- |
| `basic` | [`../projects/basic`](../projects/basic) | vue, smoke, precision | `no-v-html` |
| `configured` | [`../projects/configured`](../projects/configured) | vue, config, precision | Config discovery + active finding |
| `nuxt-graph` | [`../projects/nuxt-graph`](../projects/nuxt-graph) | nuxt, performance, precision | Graph / unused component / unresolved import |
| `vue-3.4` | [`../projects/vue-3.4`](../projects/vue-3.4) | vue, compatibility, precision | Version-gated props destructure |
| `vue-3.5` | [`../projects/vue-3.5`](../projects/vue-3.5) | vue, compatibility, precision | `prefer-use-template-ref` + unresolved import |
| `prop-flow` | [`../projects/prop-flow`](../projects/prop-flow) | vue, precision | Prop edges; whole-object / multi-hop stay quiet |
| `practice-vueuse` | [`../projects/practice-vueuse`](../projects/practice-vueuse) | vue, precision | Full VueUse practice set + `prefer-to-value` |
| `a11y-forms` | [`../projects/a11y-forms`](../projects/a11y-forms) | vue, precision | Full a11y pack TPs + named safe-pattern FPs |
| `suppressed` | [`../projects/suppressed`](../projects/suppressed) | vue, config, precision | Inline disable; `no-v-html` FP pin |
| `module-seeds` | [`../projects/module-seeds`](../projects/module-seeds) | vue, reactivity, precision | Composable seed + after-await watchEffect |
| `provide-inject` | [`../projects/provide-inject`](../projects/provide-inject) | vue, reactivity, precision | Unique-key / ambiguous provide quiet |
| `reactivity-rules` | [`../projects/reactivity-rules`](../projects/reactivity-rules) | vue, reactivity, precision | Unused / prefer-computed / conditional watchEffect |

Precision JSON uses `true_positive` / `known_limitation` for required pairs and
`false_positive` for pairs that must remain absent (safe-pattern pins).
