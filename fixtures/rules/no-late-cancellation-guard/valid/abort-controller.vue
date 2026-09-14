<script setup lang="ts">
import { ref, watch } from 'vue'
const source = ref('one')
const result = ref<string | null>(null)
watch(source, async (value, _previous, onCleanup) => {
  const controller = new AbortController()
  onCleanup(() => controller.abort())
  const data = await Promise.resolve(value)
  if (!controller.signal.aborted) result.value = data
})
</script>
<template>{{ result }}</template>
