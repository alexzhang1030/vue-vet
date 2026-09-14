<script setup lang="ts">
function watch(_source: unknown, _cb: () => void) {}
const n = { value: 0 }
watch(n.value, () => {})
</script>

<template>
  <p />
</template>
