# Require iframe titles

This high-confidence recommended rule reports a concrete Vue correctness, reactivity, performance, or accessibility failure.

## Bad

```vue
<iframe src="/preview" />
```

## Good

```vue
<iframe src="/preview" title="Document preview" />
```

## Limitations

Both static and bound titles are accepted.

## Remediation

Add a concise name for the embedded content.

