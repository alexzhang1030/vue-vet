<script setup lang="ts">
import { ref, watch } from 'vue'
const outer = ref(0)
const inner = ref(0)
const enabled = true
watch(outer, () => {
  if (enabled) {
    watch(inner, () => {}, { flush: 'sync' })
  }
}, { flush: 'sync' })
</script>
<template>{{ outer }}{{ inner }}</template>
