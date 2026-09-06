<script setup lang="ts">
import { computed, ref } from 'vue'
const output = ref(0)
const result = computed(() => {
  output.value = 1
  {
    const output = ref(2)
    output.value = 3
  }
  return 1
})
</script>
<template>{{ result }}</template>
