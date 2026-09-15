<script setup lang="ts">
const data = await fetch('/api').then((response) => response.json())
defineExpose({ data })
</script>
<template>
  <div>{{ data }}</div>
</template>
