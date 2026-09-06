<script setup lang="ts">
import { onWatcherCleanup, ref, watchEffect } from 'vue'
const source = ref(0)
watchEffect(async () => {
  source.value
  onWatcherCleanup(() => {})
  await Promise.resolve()
})
</script>
<template>{{ source }}</template>
