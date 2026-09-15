<script lang="ts">
const shared = { n: 1 }
</script>
<script setup lang="ts">
const model = defineModel({ default: () => shared })
defineExpose({ model })
</script>
<template>
  <div>{{ model.n }}</div>
</template>
