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

## Detection

The rule consumes Oxc-proven reactivity graph edges for `watchEffect`,
`watchPostEffect`, and `watchSyncEffect`. It understands sequential early
returns, if/else branches, logical short-circuiting, and ternaries. Each finding
retains all reactive guards and the exact guarded property.

Imported aliases, Vue namespace imports, explicit Nuxt imports, inline arrow and
function callbacks, and Vue ref/proxy primitives are normalized before control
flow is classified. A dependency already read unconditionally earlier in the
same effect is not reported.

## Limitations

Nested callback bodies, local lookalike functions, write-only assignment
targets, and dependencies after an `await` are excluded from this rule. The
tracer records after-await reads separately for future tracking-boundary
diagnostics.

## Remediation

Use explicit `watch` sources when every source must invalidate the side effect, or read the
required dependencies before the guard.
