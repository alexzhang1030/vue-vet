# Avoid conditionally tracked watchEffect dependencies

`watchEffect()` tracks only reactive reads reached during its synchronous execution. A reactive
read after an early-return guard is therefore not subscribed to while that guard is false.

## Bad

```vue
watchEffect(() => {
  if (!enabled.value) return
  render(result.value)
})
```

Changes to `result` do not invalidate the effect until an `enabled` change first lets execution
reach that read.

## Good

```vue
watch([enabled, result], () => {
  render(result.value)
}, { immediate: true })
```

## Limitations

The rule only reports an Oxc-proven Vue reactive binding read after an `if` early return in a
synchronous `watchEffect`, `watchPostEffect`, or `watchSyncEffect` callback. Reads already made
before the guard, reads in the guard, nested callback bodies, and write-only targets are excluded.

## Remediation

Use explicit `watch` sources when every source must invalidate the side effect, or read the
required dependencies before the guard.
