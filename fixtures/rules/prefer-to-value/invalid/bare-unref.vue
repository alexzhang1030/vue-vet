<script setup lang="ts">
const count = ref(0)
const n = unref(() => count.value)
void n
</script>

<template>
  <div />
</template>
