<script setup lang="ts">
import { onWatcherCleanup as onStop, ref, watchEffect } from 'vue'
const source = ref(0)
watchEffect(async () => {
  source.value
  await Promise.resolve()
  onStop(() => {})
})
</script>
<template>{{ source }}</template>
