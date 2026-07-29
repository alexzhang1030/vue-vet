# Require valid v-text syntax

This high-confidence recommended rule reports a concrete Vue correctness, reactivity, performance, or accessibility failure.

## Bad

```vue
<div v-text.trim="label">fallback</div>
```

## Good

```vue
<div v-text="label" />
```

## Limitations

This does not report normal interpolation.

## Remediation

Provide one expression, no argument/modifier, and no children.

