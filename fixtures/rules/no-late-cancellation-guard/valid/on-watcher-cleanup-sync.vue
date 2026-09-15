<script setup lang="ts">
import { onWatcherCleanup, ref, watch } from 'vue'
const source = ref('one')
const result = ref<string | null>(null)
watch(source, async (value) => {
  let cancelled = false
  onWatcherCleanup(() => {
    cancelled = true
  })
  const data = await Promise.resolve(value)
  if (!cancelled) result.value = data
})
</script>
<template>{{ result }}</template>
