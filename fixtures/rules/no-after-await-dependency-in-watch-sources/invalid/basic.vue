<script setup lang="ts">
import { ref, watch } from 'vue'
const count = ref(0)
watch(async () => {
  await Promise.resolve()
  return count.value
}, () => {})
</script>
<template>{{ count }}</template>
