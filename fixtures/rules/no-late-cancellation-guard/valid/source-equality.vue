<script setup lang="ts">
import { ref, watch } from 'vue'
const source = ref('one')
const result = ref<string | null>(null)
watch(source, async (value) => {
  const data = await Promise.resolve(value)
  if (source.value === value) result.value = data
})
</script>
<template>{{ result }}</template>
