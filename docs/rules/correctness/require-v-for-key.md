# Require stable v-for keys

This high-confidence recommended rule reports a concrete Vue correctness, reactivity, performance, or accessibility failure.

## Bad

```vue
<li v-for="item in items">{{ item.name }}</li>
```

## Good

```vue
<li v-for="item in items" :key="item.id">{{ item.name }}</li>
```

## Limitations

Reports only the element that owns `v-for`; it does not guess whether a key expression is semantically stable.

## Remediation

Bind a durable item identity rather than an array index.

