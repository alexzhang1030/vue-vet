<script setup lang="ts">
import { ref, watch } from 'vue'
const source = ref('one')
const result = ref<string | null>(null)
const opts = { once: true, flush: 'sync' as const }
watch(source, async (value, _previous, onCleanup) => {
  const data = await Promise.resolve(value)
  let cancelled = false
  onCleanup(() => {
    cancelled = true
  })
  if (!cancelled) result.value = data
}, opts)
</script>
<template>{{ result }}</template>
