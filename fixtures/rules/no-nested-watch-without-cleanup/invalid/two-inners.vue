<script setup lang="ts">
import { ref, watch } from 'vue'
const outer = ref(0)
const first = ref(0)
const second = ref(0)
watch(outer, () => {
  watch(first, () => {}, { flush: 'sync' })
  watch(second, () => {}, { flush: 'sync' })
}, { flush: 'sync' })
</script>
<template>{{ outer }}</template>
