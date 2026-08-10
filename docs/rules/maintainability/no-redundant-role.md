# `vue-vet/maintainability/no-redundant-role`

Category: maintainability  
Default severity: warning  
Confidence: high

Native elements already expose an implicit ARIA role; repeating it adds noise.

## Bad

```vue
<template>
  <button role="button">Save</button>
</template>
```

## Good

```vue
<template>
  <button>Save</button>
</template>
```

## Remediation

Drop the redundant `role` attribute. Static `role="…"` findings carry a safe
edit that removes the full attribute (including the quoted value).
