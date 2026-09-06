<script setup lang="ts">
import { ref, watchEffect } from 'vue'
const source = ref(0)
function onWatcherCleanup(_cleanup: () => void) {}
watchEffect(async () => {
  source.value
  await Promise.resolve()
  onWatcherCleanup(() => {})
})
</script>
<template>{{ source }}</template>
