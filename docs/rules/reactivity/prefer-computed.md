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
- one or more writes to ref-like `.value` targets
- no conditional, after-await, or outside-tracking reads

Same-file zero-arg helpers that are themselves assignment-only count as
assignment-only (`watchEffect(() => { assign() })` where `assign` only writes
refs). A local function passed by reference (`watchEffect(assign)`) is the same
assignment-only body. Side-effecting bodies (logs, DOM, network, multi-statement
control flow) stay quiet. Helpers called only from `then()` / `nextTick`, or
async / args helpers, stay quiet. Imported or method callbacks stay quiet.

## Remediation

Replace the effect with `computed(() => …)` and read the computed value where
the derived ref was used.
