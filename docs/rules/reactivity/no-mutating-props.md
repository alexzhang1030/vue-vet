# Keep props readonly

This high-confidence recommended rule reports a concrete Vue correctness, reactivity, performance, or accessibility failure.

## Bad

```vue
const props = defineProps<{ count: number }>()\nprops.count += 1
```

## Good

```vue
const props = defineProps<{ count: number }>()\nconst localCount = ref(props.count)
```

## Limitations

The first implementation recognizes the conventional `props = defineProps()` binding; destructured and aliased patterns will be expanded with later semantic rules.

## Remediation

Emit an event or copy the value into component-owned state.

