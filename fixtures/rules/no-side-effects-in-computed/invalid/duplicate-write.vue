<script setup lang="ts">
import { computed, ref } from 'vue'
const a = ref(0)
const b = ref(0)
const c = computed(() => {
  b.value = a.value
  b.value = a.value + 1
  return a.value
})
</script>
<template>{{ c }}</template>
