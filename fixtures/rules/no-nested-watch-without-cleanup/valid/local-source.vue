<script setup lang="ts">
import { ref, watch } from 'vue'
const outer = ref(0)
watch(outer, () => {
  const local = ref(0)
  watch(local, () => {}, { flush: 'sync' })
}, { flush: 'sync' })
</script>
<template>{{ outer }}</template>
