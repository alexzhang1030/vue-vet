# Avoid automatic focus

This high-confidence recommended rule reports a concrete Vue correctness, reactivity, performance, or accessibility failure.

## Bad

```vue
<input autofocus>
```

## Good

```vue
<input>
```

## Limitations

Programmatic focus is outside this template-only rule.

## Remediation

Move focus only after an explicit user action when it is necessary.

