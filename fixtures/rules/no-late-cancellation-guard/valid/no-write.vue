<script setup lang="ts">
import { ref, watch } from 'vue'
const source = ref('one')
watch(source, async (value, _previous, onCleanup) => {
  await Promise.resolve(value)
  let cancelled = false
  onCleanup(() => {
    cancelled = true
  })
  void cancelled
})
</script>
<template>{{ source }}</template>
