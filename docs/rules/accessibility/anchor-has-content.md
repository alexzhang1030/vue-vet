# Require accessible link content

This high-confidence recommended rule reports a concrete Vue correctness, reactivity, performance, or accessibility failure.

## Bad

```vue
<a href="/settings" />
```

## Good

```vue
<a href="/settings">Settings</a>
```

## Limitations

Direct child presence, `aria-label`, and `aria-labelledby` are accepted; rendered emptiness is not evaluated.

## Remediation

Add visible content or an accessible name.

