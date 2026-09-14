<script setup lang="ts">
import { onWatcherCleanup, ref, watch } from 'vue'
const source = ref('one')
const result = ref<string | null>(null)
watch(source, async (value, _previous, onCleanup) => {
  let cancelled = false
  onWatcherCleanup(() => {
    cancelled = true
  })
  const data = await Promise.resolve(value)
  onCleanup(() => {
    cancelled = true
  })
  if (!cancelled) result.value = data
})
</script>
<template>{{ result }}</template>
