<script setup lang="ts">
import { ref, watch } from 'vue'
const source = ref('one')
const result = ref<string | null>(null)
watch(source, async (value, _previous, onCleanup) => {
  let cancelled = false
  onCleanup(() => {
    cancelled = true
  })
  if (!cancelled) result.value = value
})
</script>
<template>{{ result }}</template>
