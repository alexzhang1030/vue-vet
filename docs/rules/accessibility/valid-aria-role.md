# Use concrete ARIA roles

This rule reports a static role value that contains no recognized concrete ARIA role.

## Bad

```vue
<div role="interactive-panel" />
```

## Good

```vue
<section role="region" aria-label="Status" />
```

## Limitations

Dynamic `:role` expressions are not evaluated.

## Remediation

Use a valid role only when native HTML semantics are insufficient.
