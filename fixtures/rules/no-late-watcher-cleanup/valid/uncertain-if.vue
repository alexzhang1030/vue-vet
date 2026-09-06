<script setup lang="ts">
import { onWatcherCleanup, ref, watchEffect } from 'vue'
const source = ref(0)
const ready = ref(true)
watchEffect(async () => {
  source.value
  if (ready.value) {
    await Promise.resolve()
  }
  onWatcherCleanup(() => {})
})
</script>
<template>{{ source }}</template>
