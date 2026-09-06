<script setup lang="ts">
import { getCurrentWatcher, onWatcherCleanup, ref, watchEffect } from 'vue'
const source = ref(0)
watchEffect(async () => {
  source.value
  const owner = getCurrentWatcher()
  await Promise.resolve()
  onWatcherCleanup(() => {}, false, owner)
})
</script>
<template>{{ source }}</template>
