<script setup lang="ts">
import { ref, watch } from 'vue'
const inner = ref(0)
watch(() => 1, () => {
  watch(inner, () => {}, { flush: 'sync' })
}, { immediate: true, flush: 'sync' })
</script>
<template>{{ inner }}</template>
