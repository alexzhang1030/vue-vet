# Require a dynamic component target

This high-confidence recommended rule reports a concrete Vue correctness, reactivity, performance, or accessibility failure.

## Bad

```vue
<component />
```

## Good

```vue
<component :is="currentComponent" />
```

## Limitations

Both static and bound `is` values are accepted.

## Remediation

Bind or provide the component definition/name.

