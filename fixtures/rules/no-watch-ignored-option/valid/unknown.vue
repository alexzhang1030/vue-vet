<script setup lang="ts">
function watch(_source: unknown, _cb: () => void, _options?: object) {}
const count = { value: 0 }
watch(count, () => {}, { equals: () => true })
</script>

<template>
  <p />
</template>
