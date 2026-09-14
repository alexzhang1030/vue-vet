<script setup lang="ts">
import { ref, watch as vueWatch } from 'vue'
const source = ref('one')
const result = ref<string | null>(null)
vueWatch(source, async (value, _previous, onCleanup) => {
  const data = await Promise.resolve(value)
  let cancelled = false
  onCleanup(() => {
    cancelled = true
  })
  if (!cancelled) result.value = data
})
</script>
<template>{{ result }}</template>
