<script setup lang="ts">
import { ref, watch } from 'vue'
const outer = ref(0)
const inner = ref(0)
watch(outer, () => {
  watch(inner, () => {}, { immediate: true, once: true, flush: 'sync' })
}, { flush: 'sync' })
</script>
<template>{{ outer }}{{ inner }}</template>
