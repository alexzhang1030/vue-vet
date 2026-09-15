<script setup lang="ts">
import { onWatcherCleanup, ref, watchEffect } from 'vue'
const source = ref(0)
watchEffect(async () => {
  source.value
  await Promise.resolve()
  onWatcherCleanup(() => {})
})
</script>
<template>
  <p>中文前缀</p>{{ source }}</template>
