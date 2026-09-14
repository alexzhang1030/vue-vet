<script setup lang="ts">
import { ref, watch } from 'vue'
const source = ref('one')
const result = ref<string | null>(null)
let generation = 0
watch(source, async (value) => {
  const current = (generation += 1)
  const data = await Promise.resolve(value)
  if (current === generation) result.value = data
})
</script>
<template>{{ result }}</template>
