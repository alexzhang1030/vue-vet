<script setup lang="ts">
const model = defineModel({ default: 1 })
defineExpose({ model })
</script>
<template>
  <div>{{ model }}</div>
</template>
