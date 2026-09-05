# `vue-vet/reactivity/no-conditional-dependency-in-render`

Category: reactivity
Default severity: warning
Confidence: high

**Retired.** This ID stays registered for config compatibility and never reports.

Vue tracks dynamic dependencies in render the same way as in `computed`: a reactive guard still subscribes, then later reads are picked up on the next render.

Quiet regression (must not report):

```vue
import { defineComponent, ref } from 'vue'

const enabled = ref(false)
const count = ref(0)

export default defineComponent(() => {
  return () => {
    if (!enabled.value) {
      return <p>off</p>
    }
    return <p>{count.value}</p>
  }
})
```

## Fixtures

- `fixtures/rules/no-conditional-dependency-in-render/valid/former-invalid-guarded-render.tsx`
- `fixtures/rules/no-conditional-dependency-in-render/valid/former-invalid-ident-getter.tsx`
- `fixtures/rules/no-conditional-dependency-in-render/valid/former-invalid-ident-getter.vue`
- `fixtures/rules/no-conditional-dependency-in-render/valid/ident-getter.tsx`
- `fixtures/rules/no-conditional-dependency-in-render/valid/ident-getter.vue`
- `fixtures/rules/no-conditional-dependency-in-render/valid/read-before-guard.tsx`
