# `vue-vet/reactivity/no-reactive-private-field-access`

Category: reactivity  
Default severity: warning  
Confidence: high

Vue `reactive`, `readonly`, `shallowReactive`, and `shallowReadonly` wrap an
instance in a Proxy. Native `#private` fields brand the original instance.
Calling an ordinary prototype method or reading a getter that uses
`this.#field` on that proxy throws `TypeError`.

A class that only contains private fields is not a finding. The failure is the
later executed member access whose receiver is the unbranded proxy.

## Bad

```vue
<script setup lang="ts">
import { reactive } from 'vue'
class Counter {
  #n = 1
  read() {
    return this.#n
  }
}
const proxy = reactive(new Counter())
void proxy.read()
</script>
```

## Good

```vue
<script setup lang="ts">
import { reactive, toRaw } from 'vue'
class Bound {
  #n = 1
  constructor() {
    this.read = this.read.bind(this)
  }
  read() {
    return this.#n
  }
}
class Arrow {
  #n = 2
  read = () => this.#n
}
const bound = reactive(new Bound())
void bound.read()
const arrow = reactive(new Arrow())
void arrow.read()
class RawRead {
  #n = 3
  read() {
    return toRaw(this).#n
  }
}
const raw = reactive(new RawRead())
void raw.read()
</script>
```

Import aliases, `const wrap = reactive`, proxy aliases, namespace imports,
getters, and `this` aliases inside a straight-line method are the same ID.

Quiet because the call is safe: lexical arrow fields, constructor-bound
methods, public or TypeScript `private` fields, static `#` access,
`other.#field`, `#field in this` guards, `toRaw` receivers, `markRaw`,
constructor return replacements, prototype or method replacement, own fields
that shadow the prototype member, and class fields that make Vue skip
proxying (`__v_skip`, `__v_raw`, `[Symbol.toStringTag]`).

Quiet because unproven (out of scope, not a safety claim): inheritance,
decorators, escaped classes, helper calls inside the method, `new.target`,
async methods (they reject a promise rather than throw synchronously),
`new Class()` before the class declaration (TDZ `ReferenceError`), and
capability poison (`proxy.read = …`).

Constructor-bound methods and `toRaw` keep the branded receiver, so the call
succeeds. Those raw paths are not Proxy traps: assigning `#field` does not
notify Vue.

## Detection

Fact-driven via Vue Vet source-contract facts. A class-symbol/member index
records ordinary prototype methods and getters with a reachable unconditional
`this.#field` read. Per-object operations join that index to a proven
`reactive` / `readonly` / `shallowReactive` / `shallowReadonly` allocation of
`new LocalClass()`. Main span is the failing call or getter read; related
spans identify proxy construction, member declaration, and the private access.

## Limitations

The proof is flow-insensitive. The following silence the rule even when the
member access still throws at runtime:

- Any escape of the proxy identifier, including after the demand:
  `use(proxy)`, `return proxy`, `return { proxy }`, a spread of `proxy`, a
  non-const alias (`let alias = proxy`), or an export of `proxy`.
- Any write to a key other than `.value` on that root (`proxy.other = 1`,
  `proxy[key] = 1`, `Object.defineProperty(proxy, …)`). Those mark the whole
  object capability as uncertain.

Not yet covered (also throw on Vue 3.5.40; silence is not safety):

- `ref(new C()).value.m()`
- `reactive({ c: new C() }).c.m()`
- setter assignment (`proxy.count = 2` where `set count` writes `#field`)
- private methods / private getters (`this.#peek()`, `this.#v`)
- optional call / optional object (`proxy.read?.()`, `proxy?.read()`)
- `computed(() => proxy.m())` / `watchEffect(() => { proxy.m() })`
- destructured getter (`const { count } = proxy`)

## Remediation

Keep `this` as the original instance: bind the method in the constructor, use
an arrow field, or call through `toRaw(proxy)`. Do not wrap the instance if
private branding is required. Raw access and lexical binding change
notification behavior because `#field` writes are not observed by Vue.

## Fixtures

- Invalid: `fixtures/rules/no-reactive-private-field-access/invalid/`
- Valid: `fixtures/rules/no-reactive-private-field-access/valid/`
