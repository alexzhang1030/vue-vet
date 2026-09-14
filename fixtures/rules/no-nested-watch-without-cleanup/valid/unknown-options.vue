<script setup lang="ts">
import { ref, watch } from 'vue'
const outer = ref(0)
const inner = ref(0)
const options = { flush: 'sync' as const }
watch(outer, () => {
  watch(inner, () => {}, { flush: 'sync' })
}, options)
</script>
<template>{{ outer }}{{ inner }}</template>
