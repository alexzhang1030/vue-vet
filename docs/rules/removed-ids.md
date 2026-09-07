# Removed rule IDs

These IDs are no longer registered. Delete them from `[rules]` in `vue-vet.toml`
(and from suppressions). Unknown IDs fail configuration before a scan.

Vue 3.5 `<script setup>` restores instance context across top-level `await`,
compiler macros are hoisted, Vue tracks dynamic dependencies behind a reactive
guard, and `watch*Effect` coalesces a synchronous self-write into one run.
Those premises no longer support a diagnostic. Keep
`no-define-expose-after-await` (expose still fails after `await`) and
`no-computed-self-trigger` (computed self-write is a cache/purity defect).

## After-await registrars and macros (31)

- `vue-vet/correctness/no-define-emits-after-await`
- `vue-vet/correctness/no-define-model-after-await`
- `vue-vet/correctness/no-define-options-after-await`
- `vue-vet/correctness/no-define-props-after-await`
- `vue-vet/correctness/no-define-slots-after-await`
- `vue-vet/correctness/no-effect-scope-after-await`
- `vue-vet/correctness/no-get-current-instance-after-await`
- `vue-vet/correctness/no-inject-after-await`
- `vue-vet/correctness/no-next-tick-after-await`
- `vue-vet/correctness/no-on-activated-after-await`
- `vue-vet/correctness/no-on-before-mount-after-await`
- `vue-vet/correctness/no-on-before-unmount-after-await`
- `vue-vet/correctness/no-on-before-update-after-await`
- `vue-vet/correctness/no-on-deactivated-after-await`
- `vue-vet/correctness/no-on-error-captured-after-await`
- `vue-vet/correctness/no-on-mounted-after-await`
- `vue-vet/correctness/no-on-render-tracked-after-await`
- `vue-vet/correctness/no-on-render-triggered-after-await`
- `vue-vet/correctness/no-on-server-prefetch-after-await`
- `vue-vet/correctness/no-on-unmounted-after-await`
- `vue-vet/correctness/no-on-updated-after-await`
- `vue-vet/correctness/no-provide-after-await`
- `vue-vet/correctness/no-use-attrs-after-await`
- `vue-vet/correctness/no-use-css-module-after-await`
- `vue-vet/correctness/no-use-css-vars-after-await`
- `vue-vet/correctness/no-use-slots-after-await`
- `vue-vet/correctness/no-watch-after-await`
- `vue-vet/correctness/no-watch-effect-after-await`
- `vue-vet/correctness/no-watch-post-effect-after-await`
- `vue-vet/correctness/no-watch-sync-effect-after-await`
- `vue-vet/correctness/no-with-defaults-after-await`

## Conditional-dependency family (6)

- `vue-vet/reactivity/no-conditional-dependency-in-computed`
- `vue-vet/reactivity/no-conditional-dependency-in-effect-scope`
- `vue-vet/reactivity/no-conditional-dependency-in-render`
- `vue-vet/reactivity/no-conditional-dependency-in-watch-sources`
- `vue-vet/reactivity/no-conditional-watch-effect-dependency`
- `vue-vet/reactivity/prefer-explicit-sources-for-conditional-deps`

## Effect self-trigger family (3)

- `vue-vet/reactivity/no-self-trigger-in-watch-effect`
- `vue-vet/reactivity/no-self-trigger-in-watch-post-effect`
- `vue-vet/reactivity/no-self-trigger-in-watch-sync-effect`

Semantic regressions for the Vue behavior live under
`fixtures/reactivity-semantics/`.
