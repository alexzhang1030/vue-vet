# Rule catalog

Generated from `RuleMeta` documentation keys. Regenerate with
`python3 scripts/gen_rule_catalog.py`.

Total registered rules: **136**.

| Category | Count |
| --- | ---: |
| `accessibility` | 13 |
| `correctness` | 67 |
| `maintainability` | 1 |
| `practice` | 12 |
| `reactivity` | 42 |
| `security` | 1 |

Per-rule pages live under `docs/rules/<category>/<name>.md`.

## accessibility

- [`vue-vet/accessibility/anchor-has-content`](./accessibility/anchor-has-content.md)
- [`vue-vet/accessibility/button-has-content`](./accessibility/button-has-content.md)
- [`vue-vet/accessibility/click-events-have-key-events`](./accessibility/click-events-have-key-events.md)
- [`vue-vet/accessibility/form-control-has-label`](./accessibility/form-control-has-label.md)
- [`vue-vet/accessibility/heading-has-content`](./accessibility/heading-has-content.md)
- [`vue-vet/accessibility/iframe-has-title`](./accessibility/iframe-has-title.md)
- [`vue-vet/accessibility/img-has-alt`](./accessibility/img-has-alt.md)
- [`vue-vet/accessibility/label-has-for`](./accessibility/label-has-for.md)
- [`vue-vet/accessibility/no-aria-hidden-on-focusable`](./accessibility/no-aria-hidden-on-focusable.md)
- [`vue-vet/accessibility/no-autofocus`](./accessibility/no-autofocus.md)
- [`vue-vet/accessibility/no-distracting-elements`](./accessibility/no-distracting-elements.md)
- [`vue-vet/accessibility/no-positive-tabindex`](./accessibility/no-positive-tabindex.md)
- [`vue-vet/accessibility/valid-aria-role`](./accessibility/valid-aria-role.md)

## correctness

- [`vue-vet/correctness/no-child-content`](./correctness/no-child-content.md)
- [`vue-vet/correctness/no-define-emits-after-await`](./correctness/no-define-emits-after-await.md)
- [`vue-vet/correctness/no-define-expose-after-await`](./correctness/no-define-expose-after-await.md)
- [`vue-vet/correctness/no-define-model-after-await`](./correctness/no-define-model-after-await.md)
- [`vue-vet/correctness/no-define-options-after-await`](./correctness/no-define-options-after-await.md)
- [`vue-vet/correctness/no-define-props-after-await`](./correctness/no-define-props-after-await.md)
- [`vue-vet/correctness/no-define-slots-after-await`](./correctness/no-define-slots-after-await.md)
- [`vue-vet/correctness/no-deprecated-filter`](./correctness/no-deprecated-filter.md)
- [`vue-vet/correctness/no-deprecated-slot-attribute`](./correctness/no-deprecated-slot-attribute.md)
- [`vue-vet/correctness/no-deprecated-slot-scope`](./correctness/no-deprecated-slot-scope.md)
- [`vue-vet/correctness/no-deprecated-v-bind-sync`](./correctness/no-deprecated-v-bind-sync.md)
- [`vue-vet/correctness/no-deprecated-v-on-native-modifier`](./correctness/no-deprecated-v-on-native-modifier.md)
- [`vue-vet/correctness/no-dupe-v-else-if`](./correctness/no-dupe-v-else-if.md)
- [`vue-vet/correctness/no-duplicate-attributes`](./correctness/no-duplicate-attributes.md)
- [`vue-vet/correctness/no-duplicate-define-emits`](./correctness/no-duplicate-define-emits.md)
- [`vue-vet/correctness/no-duplicate-define-expose`](./correctness/no-duplicate-define-expose.md)
- [`vue-vet/correctness/no-duplicate-define-model`](./correctness/no-duplicate-define-model.md)
- [`vue-vet/correctness/no-duplicate-define-options`](./correctness/no-duplicate-define-options.md)
- [`vue-vet/correctness/no-duplicate-define-props`](./correctness/no-duplicate-define-props.md)
- [`vue-vet/correctness/no-duplicate-define-slots`](./correctness/no-duplicate-define-slots.md)
- [`vue-vet/correctness/no-effect-scope-after-await`](./correctness/no-effect-scope-after-await.md)
- [`vue-vet/correctness/no-get-current-instance-after-await`](./correctness/no-get-current-instance-after-await.md)
- [`vue-vet/correctness/no-import-compiler-macros`](./correctness/no-import-compiler-macros.md)
- [`vue-vet/correctness/no-inject-after-await`](./correctness/no-inject-after-await.md)
- [`vue-vet/correctness/no-mutating-props`](./correctness/no-mutating-props.md)
- [`vue-vet/correctness/no-next-tick-after-await`](./correctness/no-next-tick-after-await.md)
- [`vue-vet/correctness/no-on-activated-after-await`](./correctness/no-on-activated-after-await.md)
- [`vue-vet/correctness/no-on-before-mount-after-await`](./correctness/no-on-before-mount-after-await.md)
- [`vue-vet/correctness/no-on-before-unmount-after-await`](./correctness/no-on-before-unmount-after-await.md)
- [`vue-vet/correctness/no-on-before-update-after-await`](./correctness/no-on-before-update-after-await.md)
- [`vue-vet/correctness/no-on-deactivated-after-await`](./correctness/no-on-deactivated-after-await.md)
- [`vue-vet/correctness/no-on-error-captured-after-await`](./correctness/no-on-error-captured-after-await.md)
- [`vue-vet/correctness/no-on-mounted-after-await`](./correctness/no-on-mounted-after-await.md)
- [`vue-vet/correctness/no-on-render-tracked-after-await`](./correctness/no-on-render-tracked-after-await.md)
- [`vue-vet/correctness/no-on-render-triggered-after-await`](./correctness/no-on-render-triggered-after-await.md)
- [`vue-vet/correctness/no-on-server-prefetch-after-await`](./correctness/no-on-server-prefetch-after-await.md)
- [`vue-vet/correctness/no-on-unmounted-after-await`](./correctness/no-on-unmounted-after-await.md)
- [`vue-vet/correctness/no-on-updated-after-await`](./correctness/no-on-updated-after-await.md)
- [`vue-vet/correctness/no-provide-after-await`](./correctness/no-provide-after-await.md)
- [`vue-vet/correctness/no-template-key`](./correctness/no-template-key.md)
- [`vue-vet/correctness/no-textarea-mustache`](./correctness/no-textarea-mustache.md)
- [`vue-vet/correctness/no-use-attrs-after-await`](./correctness/no-use-attrs-after-await.md)
- [`vue-vet/correctness/no-use-css-module-after-await`](./correctness/no-use-css-module-after-await.md)
- [`vue-vet/correctness/no-use-css-vars-after-await`](./correctness/no-use-css-vars-after-await.md)
- [`vue-vet/correctness/no-use-slots-after-await`](./correctness/no-use-slots-after-await.md)
- [`vue-vet/correctness/no-v-if-with-v-for`](./correctness/no-v-if-with-v-for.md)
- [`vue-vet/correctness/no-v-text-v-html-on-component`](./correctness/no-v-text-v-html-on-component.md)
- [`vue-vet/correctness/no-watch-after-await`](./correctness/no-watch-after-await.md)
- [`vue-vet/correctness/no-watch-effect-after-await`](./correctness/no-watch-effect-after-await.md)
- [`vue-vet/correctness/no-watch-post-effect-after-await`](./correctness/no-watch-post-effect-after-await.md)
- [`vue-vet/correctness/no-watch-sync-effect-after-await`](./correctness/no-watch-sync-effect-after-await.md)
- [`vue-vet/correctness/no-with-defaults-after-await`](./correctness/no-with-defaults-after-await.md)
- [`vue-vet/correctness/require-component-is`](./correctness/require-component-is.md)
- [`vue-vet/correctness/require-toggle-inside-transition`](./correctness/require-toggle-inside-transition.md)
- [`vue-vet/correctness/require-v-for-key`](./correctness/require-v-for-key.md)
- [`vue-vet/correctness/valid-v-bind`](./correctness/valid-v-bind.md)
- [`vue-vet/correctness/valid-v-else`](./correctness/valid-v-else.md)
- [`vue-vet/correctness/valid-v-else-if`](./correctness/valid-v-else-if.md)
- [`vue-vet/correctness/valid-v-for`](./correctness/valid-v-for.md)
- [`vue-vet/correctness/valid-v-html`](./correctness/valid-v-html.md)
- [`vue-vet/correctness/valid-v-if`](./correctness/valid-v-if.md)
- [`vue-vet/correctness/valid-v-memo`](./correctness/valid-v-memo.md)
- [`vue-vet/correctness/valid-v-model`](./correctness/valid-v-model.md)
- [`vue-vet/correctness/valid-v-on`](./correctness/valid-v-on.md)
- [`vue-vet/correctness/valid-v-show`](./correctness/valid-v-show.md)
- [`vue-vet/correctness/valid-v-slot`](./correctness/valid-v-slot.md)
- [`vue-vet/correctness/valid-v-text`](./correctness/valid-v-text.md)

## maintainability

- [`vue-vet/maintainability/no-redundant-role`](./maintainability/no-redundant-role.md)

## practice

- [`vue-vet/practice/prefer-define-model`](./practice/prefer-define-model.md)
- [`vue-vet/practice/prefer-to-value`](./practice/prefer-to-value.md)
- [`vue-vet/practice/prefer-use-slots-attrs`](./practice/prefer-use-slots-attrs.md)
- [`vue-vet/practice/vueuse-use-debounce-fn`](./practice/vueuse-use-debounce-fn.md)
- [`vue-vet/practice/vueuse-use-event-listener`](./practice/vueuse-use-event-listener.md)
- [`vue-vet/practice/vueuse-use-intersection-observer`](./practice/vueuse-use-intersection-observer.md)
- [`vue-vet/practice/vueuse-use-interval-fn`](./practice/vueuse-use-interval-fn.md)
- [`vue-vet/practice/vueuse-use-mutation-observer`](./practice/vueuse-use-mutation-observer.md)
- [`vue-vet/practice/vueuse-use-raf-fn`](./practice/vueuse-use-raf-fn.md)
- [`vue-vet/practice/vueuse-use-resize-observer`](./practice/vueuse-use-resize-observer.md)
- [`vue-vet/practice/vueuse-use-timeout-fn`](./practice/vueuse-use-timeout-fn.md)
- [`vue-vet/practice/vueuse-use-window-size`](./practice/vueuse-use-window-size.md)

## reactivity

- [`vue-vet/reactivity/no-after-await-dependency-in-computed`](./reactivity/no-after-await-dependency-in-computed.md)
- [`vue-vet/reactivity/no-after-await-dependency-in-effect-scope`](./reactivity/no-after-await-dependency-in-effect-scope.md)
- [`vue-vet/reactivity/no-after-await-dependency-in-watch-sources`](./reactivity/no-after-await-dependency-in-watch-sources.md)
- [`vue-vet/reactivity/no-after-await-watch-effect-dependency`](./reactivity/no-after-await-watch-effect-dependency.md)
- [`vue-vet/reactivity/no-assignment-only-effect-with-conditional-read`](./reactivity/no-assignment-only-effect-with-conditional-read.md)
- [`vue-vet/reactivity/no-computed-as-operand`](./reactivity/no-computed-as-operand.md)
- [`vue-vet/reactivity/no-computed-self-trigger`](./reactivity/no-computed-self-trigger.md)
- [`vue-vet/reactivity/no-computed-without-dependency`](./reactivity/no-computed-without-dependency.md)
- [`vue-vet/reactivity/no-conditional-dependency-in-computed`](./reactivity/no-conditional-dependency-in-computed.md)
- [`vue-vet/reactivity/no-conditional-dependency-in-effect-scope`](./reactivity/no-conditional-dependency-in-effect-scope.md)
- [`vue-vet/reactivity/no-conditional-dependency-in-watch-sources`](./reactivity/no-conditional-dependency-in-watch-sources.md)
- [`vue-vet/reactivity/no-conditional-watch-effect-dependency`](./reactivity/no-conditional-watch-effect-dependency.md)
- [`vue-vet/reactivity/no-effect-write-without-read`](./reactivity/no-effect-write-without-read.md)
- [`vue-vet/reactivity/no-empty-watch-sources`](./reactivity/no-empty-watch-sources.md)
- [`vue-vet/reactivity/no-model-ref-as-operand`](./reactivity/no-model-ref-as-operand.md)
- [`vue-vet/reactivity/no-multiple-effects-same-target`](./reactivity/no-multiple-effects-same-target.md)
- [`vue-vet/reactivity/no-nonreactive-props-destructure`](./reactivity/no-nonreactive-props-destructure.md)
- [`vue-vet/reactivity/no-on-scope-dispose-reactive-read`](./reactivity/no-on-scope-dispose-reactive-read.md)
- [`vue-vet/reactivity/no-outside-tracking-dependency-in-computed`](./reactivity/no-outside-tracking-dependency-in-computed.md)
- [`vue-vet/reactivity/no-outside-tracking-dependency-in-effect-scope`](./reactivity/no-outside-tracking-dependency-in-effect-scope.md)
- [`vue-vet/reactivity/no-outside-tracking-dependency-in-watch-sources`](./reactivity/no-outside-tracking-dependency-in-watch-sources.md)
- [`vue-vet/reactivity/no-props-snapshot-in-ref`](./reactivity/no-props-snapshot-in-ref.md)
- [`vue-vet/reactivity/no-reactive-destructure`](./reactivity/no-reactive-destructure.md)
- [`vue-vet/reactivity/no-readonly-mutation`](./reactivity/no-readonly-mutation.md)
- [`vue-vet/reactivity/no-ref-as-operand`](./reactivity/no-ref-as-operand.md)
- [`vue-vet/reactivity/no-route-destructure`](./reactivity/no-route-destructure.md)
- [`vue-vet/reactivity/no-router-destructure`](./reactivity/no-router-destructure.md)
- [`vue-vet/reactivity/no-self-trigger-in-watch-effect`](./reactivity/no-self-trigger-in-watch-effect.md)
- [`vue-vet/reactivity/no-self-trigger-in-watch-post-effect`](./reactivity/no-self-trigger-in-watch-post-effect.md)
- [`vue-vet/reactivity/no-self-trigger-in-watch-sync-effect`](./reactivity/no-self-trigger-in-watch-sync-effect.md)
- [`vue-vet/reactivity/no-shallow-reactive-destructure`](./reactivity/no-shallow-reactive-destructure.md)
- [`vue-vet/reactivity/no-side-effects-in-computed`](./reactivity/no-side-effects-in-computed.md)
- [`vue-vet/reactivity/no-stale-prop-flow`](./reactivity/no-stale-prop-flow.md)
- [`vue-vet/reactivity/no-unused-computed-binding`](./reactivity/no-unused-computed-binding.md)
- [`vue-vet/reactivity/no-unused-reactive-binding`](./reactivity/no-unused-reactive-binding.md)
- [`vue-vet/reactivity/no-v-model-nonreactive-source`](./reactivity/no-v-model-nonreactive-source.md)
- [`vue-vet/reactivity/no-watch-callback-as-tracking-scope`](./reactivity/no-watch-callback-as-tracking-scope.md)
- [`vue-vet/reactivity/prefer-computed`](./reactivity/prefer-computed.md)
- [`vue-vet/reactivity/prefer-explicit-sources-for-conditional-deps`](./reactivity/prefer-explicit-sources-for-conditional-deps.md)
- [`vue-vet/reactivity/prefer-store-to-refs`](./reactivity/prefer-store-to-refs.md)
- [`vue-vet/reactivity/prefer-use-template-ref`](./reactivity/prefer-use-template-ref.md)
- [`vue-vet/reactivity/prefer-watch-over-effect-for-single-source`](./reactivity/prefer-watch-over-effect-for-single-source.md)

## security

- [`vue-vet/security/no-v-html`](./security/no-v-html.md)

## How to read a rule

- CLI: `vue-vet --explain <rule-id>`
- Fixtures: `fixtures/rules/<name>/{invalid,valid}/`
- Practice suggestions (`category: practice`) do not affect score by default
