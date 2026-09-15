<script setup lang="ts">
const data = await fetch('/api').then((response) => response.json())
defineExpose({ data })
</script>
<template>
  <p>中文前缀</p>
  <div>{{ data }}</div>
</template>
