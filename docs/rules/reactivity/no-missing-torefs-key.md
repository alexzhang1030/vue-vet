# `vue-vet/reactivity/no-missing-torefs-key`

Category: reactivity  
Default severity: warning  
Confidence: high

`toRefs` of a reactive object with closed enumerable keys leaves missing
members as `undefined`. Dereferencing `.value` throws.

## Bad

```vue
<script setup lang="ts">
import { reactive, toRefs } from 'vue'
void toRefs(reactive({ count: 1 })).missing.value
</script>
```

## Good

```vue
<script setup lang="ts">
import { reactive, toRef, toRefs } from 'vue'
const { count } = toRefs(reactive({ count: 1 }))
void count.value
const future = toRef(reactive({ count: 1 }), 'future')
void future.value
</script>
```

Unused missing bindings, optional or guarded reads, prior return/throw
between `toRefs` and the demand, spreads, custom prototypes (`__proto__`),
Object-prototype bag keys, accessors, dynamic keys, and key mutation stay
quiet. Proven Vue `toRefs` first-argument borrows keep a closed local source;
helper arguments, aliases passed to helpers, container storage, exports,
`new`, tagged templates, and receiver calls make source keys unknown. A
method named `toRefs` is not a borrow without Vue identity. Do not warn
merely because a property is absent.

## Detection

Fact-driven via Vue Vet source-contract facts. Keys come from structured
object entries, not identifier spelling. Immediate `toRefs(...).missing.value`,
immutable local bags, and destructuring are in scope.

## Remediation

Read a key that exists, or use `toRef(object, 'future')` for a property ref.

## Fixtures

- Invalid: `fixtures/rules/no-missing-torefs-key/invalid/`
- Valid: `fixtures/rules/no-missing-torefs-key/valid/`
