# Replace removed slot-scope syntax

Vue 3 uses `v-slot` and the `#` shorthand instead of `slot-scope` or template `scope`.

## Bad

```vue
<template slot-scope="slotProps">{{ slotProps.value }}</template>
```

## Good

```vue
<template #default="slotProps">{{ slotProps.value }}</template>
```

## Remediation

Migrate scoped-slot declarations to `v-slot`.
