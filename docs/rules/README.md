# Rule catalog

Generated from `RuleMeta` documentation keys. Regenerate with
`python3 scripts/gen_rule_catalog.py` (`just rules-catalog`).

## Differentiation tiers

Vue Vet ships a large built-in set. **Differentiation is the reactivity tracer**,
not Essential/a11y parity with `eslint-plugin-vue`.

| Tier | Meaning | Count |
| --- | --- | ---: |
| `tracer` | Needs `vue_vet_reactivity` graph facts (read kinds, guards, scopes, prop edges, binding kinds) | 50 |
| `parity` | Template Essential / a11y / macros / after-await registrars — open-box completeness | 50 |
| `practice` | Ecosystem suggestions (`category: practice`); excluded from score by default | 13 |

Total registered **file** rules (builtins + practice): **113**.

Project-graph IDs are listed separately below and are not in this file-ID set.

| Category | Count |
| --- | ---: |
| `accessibility` | 13 |
| `correctness` | 36 |
| `maintainability` | 1 |
| `practice` | 12 |
| `reactivity` | 50 |
| `security` | 1 |

Per-rule pages live under `docs/rules/<category>/<name>.md`.

## accessibility

- [`vue-vet/accessibility/anchor-has-content`](./accessibility/anchor-has-content.md) `parity`
- [`vue-vet/accessibility/button-has-content`](./accessibility/button-has-content.md) `parity`
- [`vue-vet/accessibility/click-events-have-key-events`](./accessibility/click-events-have-key-events.md) `parity`
- [`vue-vet/accessibility/form-control-has-label`](./accessibility/form-control-has-label.md) `parity`
- [`vue-vet/accessibility/heading-has-content`](./accessibility/heading-has-content.md) `parity`
- [`vue-vet/accessibility/iframe-has-title`](./accessibility/iframe-has-title.md) `parity`
- [`vue-vet/accessibility/img-has-alt`](./accessibility/img-has-alt.md) `parity`
- [`vue-vet/accessibility/label-has-for`](./accessibility/label-has-for.md) `parity`
- [`vue-vet/accessibility/no-aria-hidden-on-focusable`](./accessibility/no-aria-hidden-on-focusable.md) `parity`
- [`vue-vet/accessibility/no-autofocus`](./accessibility/no-autofocus.md) `parity`
- [`vue-vet/accessibility/no-distracting-elements`](./accessibility/no-distracting-elements.md) `parity`
- [`vue-vet/accessibility/no-positive-tabindex`](./accessibility/no-positive-tabindex.md) `parity`
- [`vue-vet/accessibility/valid-aria-role`](./accessibility/valid-aria-role.md) `parity`

## correctness

- [`vue-vet/correctness/no-child-content`](./correctness/no-child-content.md) `parity`
- [`vue-vet/correctness/no-define-expose-after-await`](./correctness/no-define-expose-after-await.md) `parity`
- [`vue-vet/correctness/no-deprecated-filter`](./correctness/no-deprecated-filter.md) `parity`
- [`vue-vet/correctness/no-deprecated-slot-attribute`](./correctness/no-deprecated-slot-attribute.md) `parity`
- [`vue-vet/correctness/no-deprecated-slot-scope`](./correctness/no-deprecated-slot-scope.md) `parity`
- [`vue-vet/correctness/no-deprecated-v-bind-sync`](./correctness/no-deprecated-v-bind-sync.md) `parity`
- [`vue-vet/correctness/no-deprecated-v-on-native-modifier`](./correctness/no-deprecated-v-on-native-modifier.md) `parity`
- [`vue-vet/correctness/no-dupe-v-else-if`](./correctness/no-dupe-v-else-if.md) `parity`
- [`vue-vet/correctness/no-duplicate-attributes`](./correctness/no-duplicate-attributes.md) `parity`
- [`vue-vet/correctness/no-duplicate-define-emits`](./correctness/no-duplicate-define-emits.md) `parity`
- [`vue-vet/correctness/no-duplicate-define-expose`](./correctness/no-duplicate-define-expose.md) `parity`
- [`vue-vet/correctness/no-duplicate-define-model`](./correctness/no-duplicate-define-model.md) `parity`
- [`vue-vet/correctness/no-duplicate-define-options`](./correctness/no-duplicate-define-options.md) `parity`
- [`vue-vet/correctness/no-duplicate-define-props`](./correctness/no-duplicate-define-props.md) `parity`
- [`vue-vet/correctness/no-duplicate-define-slots`](./correctness/no-duplicate-define-slots.md) `parity`
- [`vue-vet/correctness/no-import-compiler-macros`](./correctness/no-import-compiler-macros.md) `parity`
- [`vue-vet/correctness/no-mutating-props`](./correctness/no-mutating-props.md) `tracer`
- [`vue-vet/correctness/no-template-key`](./correctness/no-template-key.md) `parity`
- [`vue-vet/correctness/no-textarea-mustache`](./correctness/no-textarea-mustache.md) `parity`
- [`vue-vet/correctness/no-v-if-with-v-for`](./correctness/no-v-if-with-v-for.md) `parity`
- [`vue-vet/correctness/no-v-text-v-html-on-component`](./correctness/no-v-text-v-html-on-component.md) `parity`
- [`vue-vet/correctness/require-component-is`](./correctness/require-component-is.md) `parity`
- [`vue-vet/correctness/require-toggle-inside-transition`](./correctness/require-toggle-inside-transition.md) `parity`
- [`vue-vet/correctness/require-v-for-key`](./correctness/require-v-for-key.md) `parity`
- [`vue-vet/correctness/valid-v-bind`](./correctness/valid-v-bind.md) `parity`
- [`vue-vet/correctness/valid-v-else`](./correctness/valid-v-else.md) `parity`
- [`vue-vet/correctness/valid-v-else-if`](./correctness/valid-v-else-if.md) `parity`
- [`vue-vet/correctness/valid-v-for`](./correctness/valid-v-for.md) `parity`
- [`vue-vet/correctness/valid-v-html`](./correctness/valid-v-html.md) `parity`
- [`vue-vet/correctness/valid-v-if`](./correctness/valid-v-if.md) `parity`
- [`vue-vet/correctness/valid-v-memo`](./correctness/valid-v-memo.md) `parity`
- [`vue-vet/correctness/valid-v-model`](./correctness/valid-v-model.md) `parity`
- [`vue-vet/correctness/valid-v-on`](./correctness/valid-v-on.md) `parity`
- [`vue-vet/correctness/valid-v-show`](./correctness/valid-v-show.md) `parity`
- [`vue-vet/correctness/valid-v-slot`](./correctness/valid-v-slot.md) `parity`
- [`vue-vet/correctness/valid-v-text`](./correctness/valid-v-text.md) `parity`

## maintainability

- [`vue-vet/maintainability/no-redundant-role`](./maintainability/no-redundant-role.md) `parity`

## practice

- [`vue-vet/practice/prefer-define-model`](./practice/prefer-define-model.md) `practice`
- [`vue-vet/practice/prefer-to-value`](./practice/prefer-to-value.md) `practice`
- [`vue-vet/practice/prefer-use-slots-attrs`](./practice/prefer-use-slots-attrs.md) `practice`
- [`vue-vet/practice/vueuse-use-debounce-fn`](./practice/vueuse-use-debounce-fn.md) `practice`
- [`vue-vet/practice/vueuse-use-event-listener`](./practice/vueuse-use-event-listener.md) `practice`
- [`vue-vet/practice/vueuse-use-intersection-observer`](./practice/vueuse-use-intersection-observer.md) `practice`
- [`vue-vet/practice/vueuse-use-interval-fn`](./practice/vueuse-use-interval-fn.md) `practice`
- [`vue-vet/practice/vueuse-use-mutation-observer`](./practice/vueuse-use-mutation-observer.md) `practice`
- [`vue-vet/practice/vueuse-use-raf-fn`](./practice/vueuse-use-raf-fn.md) `practice`
- [`vue-vet/practice/vueuse-use-resize-observer`](./practice/vueuse-use-resize-observer.md) `practice`
- [`vue-vet/practice/vueuse-use-timeout-fn`](./practice/vueuse-use-timeout-fn.md) `practice`
- [`vue-vet/practice/vueuse-use-window-size`](./practice/vueuse-use-window-size.md) `practice`

## reactivity

- [`vue-vet/reactivity/no-after-await-dependency-in-computed`](./reactivity/no-after-await-dependency-in-computed.md) `tracer`
- [`vue-vet/reactivity/no-after-await-dependency-in-effect-scope`](./reactivity/no-after-await-dependency-in-effect-scope.md) `tracer`
- [`vue-vet/reactivity/no-after-await-dependency-in-watch-sources`](./reactivity/no-after-await-dependency-in-watch-sources.md) `tracer`
- [`vue-vet/reactivity/no-after-await-watch-effect-dependency`](./reactivity/no-after-await-watch-effect-dependency.md) `tracer`
- [`vue-vet/reactivity/no-assignment-only-effect-with-conditional-read`](./reactivity/no-assignment-only-effect-with-conditional-read.md) `tracer`
- [`vue-vet/reactivity/no-computed-as-operand`](./reactivity/no-computed-as-operand.md) `tracer`
- [`vue-vet/reactivity/no-computed-self-trigger`](./reactivity/no-computed-self-trigger.md) `tracer`
- [`vue-vet/reactivity/no-computed-without-dependency`](./reactivity/no-computed-without-dependency.md) `tracer`
- [`vue-vet/reactivity/no-deep-watch-on-reactive-root`](./reactivity/no-deep-watch-on-reactive-root.md) `tracer`
- [`vue-vet/reactivity/no-deferred-callback-reactive-read-in-effect`](./reactivity/no-deferred-callback-reactive-read-in-effect.md) `tracer`
- [`vue-vet/reactivity/no-effect-write-without-read`](./reactivity/no-effect-write-without-read.md) `tracer`
- [`vue-vet/reactivity/no-empty-watch-sources`](./reactivity/no-empty-watch-sources.md) `tracer`
- [`vue-vet/reactivity/no-late-scope-dispose`](./reactivity/no-late-scope-dispose.md) `tracer`
- [`vue-vet/reactivity/no-late-watcher-cleanup`](./reactivity/no-late-watcher-cleanup.md) `tracer`
- [`vue-vet/reactivity/no-lost-shallow-nested-notification`](./reactivity/no-lost-shallow-nested-notification.md) `tracer`
- [`vue-vet/reactivity/no-model-ref-as-operand`](./reactivity/no-model-ref-as-operand.md) `tracer`
- [`vue-vet/reactivity/no-multiple-effects-same-target`](./reactivity/no-multiple-effects-same-target.md) `tracer`
- [`vue-vet/reactivity/no-nonreactive-props-destructure`](./reactivity/no-nonreactive-props-destructure.md) `tracer`
- [`vue-vet/reactivity/no-on-scope-dispose-reactive-read`](./reactivity/no-on-scope-dispose-reactive-read.md) `tracer`
- [`vue-vet/reactivity/no-orphaned-scope-watcher`](./reactivity/no-orphaned-scope-watcher.md) `tracer`
- [`vue-vet/reactivity/no-outside-tracking-dependency-in-computed`](./reactivity/no-outside-tracking-dependency-in-computed.md) `tracer`
- [`vue-vet/reactivity/no-outside-tracking-dependency-in-effect-scope`](./reactivity/no-outside-tracking-dependency-in-effect-scope.md) `tracer`
- [`vue-vet/reactivity/no-outside-tracking-dependency-in-watch-sources`](./reactivity/no-outside-tracking-dependency-in-watch-sources.md) `tracer`
- [`vue-vet/reactivity/no-primitive-reactive-target`](./reactivity/no-primitive-reactive-target.md) `tracer`
- [`vue-vet/reactivity/no-props-snapshot-in-ref`](./reactivity/no-props-snapshot-in-ref.md) `tracer`
- [`vue-vet/reactivity/no-reactive-destructure`](./reactivity/no-reactive-destructure.md) `tracer`
- [`vue-vet/reactivity/no-reactive-read-during-pause-tracking`](./reactivity/no-reactive-read-during-pause-tracking.md) `tracer`
- [`vue-vet/reactivity/no-readonly-mutation`](./reactivity/no-readonly-mutation.md) `tracer`
- [`vue-vet/reactivity/no-ref-as-operand`](./reactivity/no-ref-as-operand.md) `tracer`
- [`vue-vet/reactivity/no-returned-watcher-cleanup`](./reactivity/no-returned-watcher-cleanup.md) `tracer`
- [`vue-vet/reactivity/no-route-destructure`](./reactivity/no-route-destructure.md) `tracer`
- [`vue-vet/reactivity/no-router-destructure`](./reactivity/no-router-destructure.md) `tracer`
- [`vue-vet/reactivity/no-shallow-reactive-destructure`](./reactivity/no-shallow-reactive-destructure.md) `tracer`
- [`vue-vet/reactivity/no-side-effects-in-computed`](./reactivity/no-side-effects-in-computed.md) `tracer`
- [`vue-vet/reactivity/no-stale-prop-flow`](./reactivity/no-stale-prop-flow.md) `tracer`
- [`vue-vet/reactivity/no-toraw-write-of-tracked-state`](./reactivity/no-toraw-write-of-tracked-state.md) `tracer`
- [`vue-vet/reactivity/no-torefs-on-non-proxy`](./reactivity/no-torefs-on-non-proxy.md) `tracer`
- [`vue-vet/reactivity/no-trigger-ref-on-non-ref`](./reactivity/no-trigger-ref-on-non-ref.md) `tracer`
- [`vue-vet/reactivity/no-unused-computed-binding`](./reactivity/no-unused-computed-binding.md) `tracer`
- [`vue-vet/reactivity/no-unused-reactive-binding`](./reactivity/no-unused-reactive-binding.md) `tracer`
- [`vue-vet/reactivity/no-v-model-nonreactive-source`](./reactivity/no-v-model-nonreactive-source.md) `tracer`
- [`vue-vet/reactivity/no-watch-callback-as-tracking-scope`](./reactivity/no-watch-callback-as-tracking-scope.md) `tracer`
- [`vue-vet/reactivity/no-watch-ignored-option`](./reactivity/no-watch-ignored-option.md) `tracer`
- [`vue-vet/reactivity/no-watch-replaced-object-source`](./reactivity/no-watch-replaced-object-source.md) `tracer`
- [`vue-vet/reactivity/no-watch-signature-mismatch`](./reactivity/no-watch-signature-mismatch.md) `tracer`
- [`vue-vet/reactivity/no-watch-unwrapped-source`](./reactivity/no-watch-unwrapped-source.md) `tracer`
- [`vue-vet/reactivity/prefer-computed`](./reactivity/prefer-computed.md) `tracer`
- [`vue-vet/reactivity/prefer-store-to-refs`](./reactivity/prefer-store-to-refs.md) `tracer`
- [`vue-vet/reactivity/prefer-use-template-ref`](./reactivity/prefer-use-template-ref.md) `practice`
- [`vue-vet/reactivity/prefer-watch-over-effect-for-single-source`](./reactivity/prefer-watch-over-effect-for-single-source.md) `tracer`

## security

- [`vue-vet/security/no-v-html`](./security/no-v-html.md) `parity`

## How to read a rule

- CLI: `vue-vet --explain <rule-id>`
- Fixtures: `fixtures/rules/<name>/{invalid,valid}/`
- Practice suggestions do not affect score by default
- Prefer tracer-tier findings when evaluating Vue Vet against other doctors
- Removed historical IDs: [`docs/rules/removed-ids.md`](./removed-ids.md)

## Project-graph rules

These IDs belong to the project registry and are listed separately from file rules.

- `vue-vet/project/unresolved-import`
- `vue-vet/project/unused-component`
