<script setup lang="ts">
import { getCurrentWatcher, onWatcherCleanup, watchEffect } from 'vue'
watchEffect(async () => {
  const owner = getCurrentWatcher()
  const dispose = () => {}
  await Promise.resolve()
  onWatcherCleanup(dispose, false, owner)
  return dispose
})
</script>
<template>ok</template>
