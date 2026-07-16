# Require accessible button content

This high-confidence recommended rule reports a concrete Vue correctness, reactivity, performance, or accessibility failure.

## Bad

```vue
<button type="button" />
```

## Good

```vue
<button type="button">Save</button>
```

## Limitations

Direct child presence, `aria-label`, and `aria-labelledby` are accepted.

## Remediation

Add visible content or an accessible name.

