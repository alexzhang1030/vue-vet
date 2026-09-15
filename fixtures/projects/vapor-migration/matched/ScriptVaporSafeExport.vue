<script vapor lang="ts">
// export default { name: 'commented' }
const label = "export default"
export type Foo = string
const n = ref(0)
</script>
<template>
  <p>{{ label }}{{ n }}</p>
</template>
