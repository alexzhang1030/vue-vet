<script setup lang="ts">
import { reactive, ref, watch } from 'vue'
const source = ref('one')
const results = reactive<Record<string, string>>({})
watch(source, async (value) => {
  const data = await Promise.resolve(value)
  results[value] = data
})
</script>
<template>{{ results }}</template>
