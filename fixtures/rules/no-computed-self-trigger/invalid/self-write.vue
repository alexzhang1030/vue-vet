<script setup lang="ts">
import { computed, ref } from 'vue'
const count = ref(0)
const result = computed(() => {
  count.value = count.value + 1
  return count.value
})
</script>
<template>{{ result }}</template>
