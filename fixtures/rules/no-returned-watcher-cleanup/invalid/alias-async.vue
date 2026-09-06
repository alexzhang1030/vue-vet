<script setup lang="ts">
import { ref, watchEffect as observe } from 'vue'
const source = ref(0)
observe(async () => {
  source.value
  return () => {}
})
</script>
<template>{{ source }}</template>
