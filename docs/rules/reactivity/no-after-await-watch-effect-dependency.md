# Avoid untracked reads after await in watchEffect

`watchEffect()` only tracks reactive reads reached during its **synchronous**
execution. After a top-level `await`, or inside deferred callbacks such as
`Promise.then` / `nextTick`, Vue stops collecting dependencies. Reads in those
regions do not subscribe the effect.

## Bad

```vue
watchEffect(async () => {
  await load()
  render(result.value)
})
```

```vue
watchEffect(() => {
  Promise.resolve().then(() => {
    render(result.value)
  })
})
```

Changes to `result` do not re-run the effect once execution has crossed the
async boundary.

## Good

```vue
watch(result, async () => {
  await load()
  render(result.value)
}, { immediate: true })
```

```vue
watchEffect(async () => {
  const current = result.value
  await load()
  render(current)
})
```

## Detection

The rule consumes reactivity graph edges for `watchEffect`, `watchPostEffect`,
and `watchSyncEffect`. It reports reads classified as `after_await` or
`outside_tracking` (deferred callbacks nested in the effect).

Nested arbitrary functions that are not known deferred APIs remain excluded so
the rule stays high-confidence.

## Remediation

Read required dependencies before the first `await`, or switch to explicit
`watch` sources when async work must re-run on those inputs.
