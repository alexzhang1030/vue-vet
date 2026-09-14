<script setup lang="ts">
import { ref, watch } from 'vue'
const source = ref('one')
const result = ref<string | null>(null)
const load = async (value: string, _previous: string | undefined, onCleanup: (fn: () => void) => void) => {
  const data = await Promise.resolve(value)
  let cancelled = false
  onCleanup(() => {
    cancelled = true
  })
  if (!cancelled) result.value = data
}
watch(source, load)
</script>
<template>{{ result }}</template>
