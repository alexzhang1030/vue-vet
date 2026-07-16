# Require valid v-html syntax

This high-confidence recommended rule reports a concrete Vue correctness, reactivity, performance, or accessibility failure.

## Bad

```vue
<div v-html.foo>fallback</div>
```

## Good

```vue
<div v-html="trustedHtml" />
```

## Limitations

This validates directive shape; `security/no-v-html` separately reports the trust risk.

## Remediation

Provide one expression, no argument/modifier, and no children.

