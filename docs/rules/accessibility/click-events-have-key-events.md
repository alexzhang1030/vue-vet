# Pair click handlers with keyboard behavior

This high-confidence recommended rule reports a concrete Vue correctness, reactivity, performance, or accessibility failure.

## Bad

```vue
<div @click="activate" />
```

## Good

```vue
<button type="button" @click="activate">Activate</button>
```

## Limitations

The rule is intentionally limited to common native non-interactive tags to avoid guessing custom-component semantics.

## Remediation

Prefer a native control; otherwise add keyboard handling and a role.

