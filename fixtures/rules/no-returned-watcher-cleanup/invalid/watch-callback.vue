<script setup lang="ts">
import { ref, watch } from 'vue'
const source = ref(0)
watch(source, () => {
  return () => {}
})
</script>
<template>{{ source }}</template>
