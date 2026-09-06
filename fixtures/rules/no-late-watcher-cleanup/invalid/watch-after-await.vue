<script setup lang="ts">
import { onWatcherCleanup, ref, watch } from 'vue'
const source = ref(0)
watch(source, async () => {
  await Promise.resolve()
  onWatcherCleanup(() => {})
})
</script>
<template>{{ source }}</template>
