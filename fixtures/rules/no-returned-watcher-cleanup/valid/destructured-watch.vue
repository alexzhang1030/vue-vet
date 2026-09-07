<script setup lang="ts">
import { ref, watch } from 'vue'
const source = ref({ x: 0 })
watch(
  source,
  ({ x }, old, onCleanup) => {
    const dispose = () => {}
    onCleanup(dispose)
    return dispose
  },
  { immediate: true },
)
</script>
<template>{{ source.x }}</template>
