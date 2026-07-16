# Prefer useTemplateRef on Vue 3.5 and newer

Vue 3.5 provides `useTemplateRef()` to connect a script binding to a static template ref without
manually mirroring the key with `ref(null)`.

## Bad

```vue
<script setup>
const input = ref(null)
</script>
<template>
  <input ref="input">
</template>
```

## Good

```vue
<script setup>
const input = useTemplateRef('input')
</script>
<template>
  <input ref="input">
</template>
```

## Limitations

The rule requires Vue 3.5+, a static template `ref` value, a same-named setup binding initialized
by Vue's `ref(null)`, and semantic import resolution. Dynamic and nonmatching refs stay quiet.

## Remediation

Import `useTemplateRef` from Vue and pass the template ref key.
