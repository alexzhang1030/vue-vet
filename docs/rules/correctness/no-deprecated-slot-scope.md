# `vue-vet/correctness/no-deprecated-slot-scope`

Category: correctness  
Default severity: warning  
Confidence: high

`slot-scope` is Vue 2 syntax. Prefer `v-slot` / `#` destructuring.

## Bad

```vue
<template>
  <Comp><div slot-scope="row">{{ row.name }}</div></Comp>
</template>
```

## Good

```vue
<template>
  <Comp><template #default="{ name }">{{ name }}</template></Comp>
</template>
```

## Remediation

Migrate to `v-slot` / named slot shorthand with props destructuring.
