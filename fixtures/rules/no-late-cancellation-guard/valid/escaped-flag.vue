<script setup lang="ts">
import { ref, watch } from 'vue'
const source = ref('one')
const result = ref<string | null>(null)
function hold(_flag: boolean) {}
watch(source, async (value, _previous, onCleanup) => {
  const data = await Promise.resolve(value)
  let cancelled = false
  hold(cancelled)
  onCleanup(() => {
    cancelled = true
  })
  if (!cancelled) result.value = data
})
</script>
<template>{{ result }}</template>
