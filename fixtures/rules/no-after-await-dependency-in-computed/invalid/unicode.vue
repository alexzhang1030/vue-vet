<script setup lang="ts">
import { computed, ref } from 'vue'
const a = ref(0)
const c = computed(async () => {
  await Promise.resolve()
  return a.value
})
</script>
<template>
  <p>中文前缀</p>{{ c }}</template>
