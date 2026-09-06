# Require stable v-for keys

This high-confidence recommended rule reports a concrete Vue correctness, reactivity, performance, or accessibility failure.

## Bad

```vue
<li v-for="item in items">{{ item.name }}</li>
```

## Good

```vue
<li v-for="item in items" :key="item.id">{{ item.name }}</li>
<slot v-for="item in items" v-bind="{ key: item.id }" :item="item" />
```

Proven object-form `v-bind="{ key: … }"` (literal `key` after any spreads, including parentheses and TypeScript `as` / `satisfies` wrappers) counts as a key. Statically known sibling keys such as `1` or `[2]` are distinct and do not unprove `key`. Opaque spreads **or unknown computed properties** after `key` can overwrite it and stay conservative.

## Limitations

Reports only the element that owns `v-for`; it does not guess whether a key expression is semantically stable.

## Remediation

Bind a durable item identity rather than an array index.

