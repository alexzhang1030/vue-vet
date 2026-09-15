<script setup lang="ts">
import { ref, watch } from 'vue'
const count = ref(0)
watch(() => {
  Promise.resolve().then(() => count.value)
  return 0
}, () => {})
</script>
<template>{{ count }}</template>
