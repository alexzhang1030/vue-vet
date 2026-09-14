<script setup lang="ts">
import { ref, watchEffect } from 'vue'
const source = ref('one')
const result = ref<string | null>(null)
watchEffect(async (onCleanup) => {
  const data = await Promise.resolve(source.value)
  let cancelled = false
  onCleanup(() => {
    cancelled = true
  })
  if (!cancelled) result.value = data
})
</script>
<template>{{ result }}</template>
