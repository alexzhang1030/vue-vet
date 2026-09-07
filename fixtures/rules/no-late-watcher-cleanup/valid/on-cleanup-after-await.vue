<script setup lang="ts">
import { ref, watchEffect } from 'vue'
const source = ref(0)
watchEffect(async (onCleanup) => {
  source.value
  await Promise.resolve()
  onCleanup(() => {})
})
</script>
<template>{{ source }}</template>
