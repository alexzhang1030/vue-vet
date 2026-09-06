# Prefer computed for pure derived state

When a `watchEffect` only assigns ref values from other reactive reads—with no
control flow, awaits, or other statements—it is manually re-implementing what
`computed` already does: cache a pure derivation and re-run when dependencies
change.

## Bad

```vue
const first = ref('Ada')
const last = ref('Lovelace')
const fullName = ref('')

watchEffect(() => {
  fullName.value = `${first.value} ${last.value}`
})
```

## Good

```vue
const first = ref('Ada')
const last = ref('Lovelace')
const fullName = computed(() => `${first.value} ${last.value}`)
```

## Detection

The rule inspects `watchEffect` / `watchPostEffect` / `watchSyncEffect` scopes
whose body is **assignment-only**, with:

- at least one unconditional reactive read of a binding that is not written
- one or more writes to an **owned ordinary `ref` / `shallowRef`** `.value`
  (a reactive object's `value` property, `defineModel` / ModelRef, or computed
  setter is not a conversion target)
- no conditional, after-await, or outside-tracking reads

Same-file zero-arg helpers that are themselves assignment-only count as
assignment-only (`watchEffect(() => { assign() })` where `assign` only writes
refs). A local function passed by reference (`watchEffect(assign)`) is the same
assignment-only body. `target.value = count.value` (distinct source and
destination) is a true positive.

The rule stays quiet when a tracked read and a write resolve to the same
reactive source after alias resolution (`const alias = count`, including
ordinary `<script>` / `<script setup>` same-name isolation). Compound
`total.value += n.value` overlaps `total` and is not a pure derivation.
Follow coverage must be complete (`unknown_calls` / `uncertain_accesses` /
`follow_truncated` empty); `target.value = external(count.value)` abstains.
The written binding must be a **private derived target**: the reactivity graph
must contain the written name (or an `alias_of` alias) as a proven owned local,
matched by Oxc `binding_span` / `alias_of_span` rather than display name.
An empty alias lookup abstains — writable `Ref` parameters belong to the caller
and keep a mutable contract, including when they shadow a module-level `ref`. No other writer scopes (helper writes already
attributed to this effect are the same writer), no composable/call-argument
escape, no object/array/return/export escape, no template `v-model` / `v-on`
use of the binding (or its `alias_of` aliases). Custom event names are
arbitrary; `v-on` expressions already carry `surface == "on"` plus identifier
facts. A string that contains a binding name is not an identifier use.
A ref passed to a toggle helper, stored in `{ target }`, or written from a
click handler stays a mutable UI role.
Side-effecting bodies (logs, DOM, network, multi-statement control flow) stay
quiet. Helpers called only from `then()` / `nextTick`, or async / args
helpers, stay quiet. Imported or method callbacks stay quiet.

## Remediation

Replace the effect with `computed(() => …)` and read the computed value where
the derived ref was used.

## Fixtures

- Invalid: `fixtures/rules/prefer-computed/invalid/`
- Valid: `fixtures/rules/prefer-computed/valid/` (includes `compound-from-other.vue`, `mutable-ui-target.vue`, `container-escape.vue`, `alias-container-escape.vue`, `custom-event-target.vue`, `reactive-value-property.vue`, `managed-model-target.vue`, `shadowed-ref-parameter.vue`)
