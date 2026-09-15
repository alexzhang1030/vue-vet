<script setup lang="ts">
const model = defineModel({ default: { n: 1 } })
defineExpose({ model })
</script>
<template>
  <div>{{ model.n }}</div>
</template>
