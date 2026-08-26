# `vue-vet/reactivity/no-conditional-dependency-in-render`

Category: reactivity  
Default severity: warning  
Confidence: high  
Tier: tracer

Reports reactive reads inside a component `render` function (options `render`,
`setup` → render, a same-file `render: renderFn` / `return renderFn` identifier,
or recognized functional / JSX render) that happen only after a control-flow
guard. Those reads are not stable dependencies for the render tracking scope.

Guard shape (early-exit, short-circuit, switch, branch) is recorded on the
read's fact for tooling; it does not split into separate rule ids.

## Bad

```tsx
import { defineComponent, ref } from 'vue'

const enabled = ref(false)
const count = ref(0)

export default defineComponent(() => {
  return () => {
    if (!enabled.value) return <p>off</p>
    return <p>{count.value}</p>
  }
})
```

## Good

```tsx
import { defineComponent, ref } from 'vue'

const enabled = ref(false)
const count = ref(0)

export default defineComponent(() => {
  return () => {
    const n = count.value
    if (!enabled.value) return <p>off</p>
    return <p>{n}</p>
  }
})
```

A same-file `render: renderFn` is the same tracking body:

```ts
function renderFn() {
  return <p>{count.value}</p>
}
export default defineComponent({ render: renderFn })
```

## Detection

Fact-driven: `TrackingScopeKind::Render` with `ReactiveReadKind::Conditional`.

## Remediation

Read each needed dependency synchronously before guards, or move derivation into
`computed` / explicit `watch` sources.

## Fixtures

- Invalid: `fixtures/rules/no-conditional-dependency-in-render/invalid/`
- Valid: `fixtures/rules/no-conditional-dependency-in-render/valid/`
