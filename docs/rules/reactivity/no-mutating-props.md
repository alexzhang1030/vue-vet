# Keep props readonly

This high-confidence recommended rule reports a concrete Vue correctness, reactivity, performance, or accessibility failure.

## Bad

```vue
const componentProps = defineProps<{ count: number }>()\ncomponentProps.count += 1
```

## Good

```vue
const props = defineProps<{ count: number }>()\nconst localCount = ref(props.count)
```

## Limitations

The rule follows any identifier directly assigned from `defineProps()`. Destructured props are not treated as mutable objects.

## Remediation

Emit an event or copy the value into component-owned state.

