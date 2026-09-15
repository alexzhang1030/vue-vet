<script setup lang="ts">
import { ref, watch } from 'vue'
const outer = ref(0)
const inner = ref(0)
watch(
  outer,
  () => {
    watch(inner, () => {}, { flush: 'sync' })
  },
  { flush: 'sync', immediate: true },
)
</script>
<template>{{ outer }}{{ inner }}</template>
