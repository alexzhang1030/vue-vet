# `vue-vet/accessibility/valid-aria-role`

Category: accessibility  
Default severity: warning  
Confidence: high

`role` values must be valid ARIA roles.

## Bad

```vue
<template>
  <div role="primry">...</div>
</template>
```

## Good

```vue
<template>
  <div role="status">...</div>
</template>
```

## Remediation

Correct the typo or remove an invalid custom role.
