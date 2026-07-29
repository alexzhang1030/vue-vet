# Disallow duplicate conditions in `v-else-if` chains

This high-confidence rule reports a `v-else-if` whose condition string repeats the immediately preceding `v-else-if` branch. The later branch is unreachable because the earlier, identical condition already matched.

## Bad

```vue
<div v-if="type === 'a'">A</div>
<div v-else-if="type === 'b'">B</div>
<div v-else-if="type === 'b'">unreachable</div>
```

## Good

```vue
<div v-if="type === 'a'">A</div>
<div v-else-if="type === 'b'">B</div>
<div v-else-if="type === 'c'">C</div>
```

## Limitations

Facts expose a flat, parent-before-child element list with no explicit sibling links. This rule only compares directly adjacent flat entries, which holds for the common case of a leaf-element `v-if`/`v-else-if` chain, but under-approximates chains whose earlier branches contain nested elements (the adjacency breaks once a branch has descendant elements between it and its sibling).

## Remediation

Remove the duplicated branch, or fix its condition.
