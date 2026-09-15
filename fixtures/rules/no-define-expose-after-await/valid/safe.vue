<script setup lang="ts">
defineExpose()
const data = await fetch('/api').then((response) => response.json())
</script>
<template>
  <div>{{ data }}</div>
</template>
