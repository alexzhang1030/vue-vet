<script setup lang="ts">
import { ref, watch } from 'vue'
const outer = ref(0)
const inner = ref(0)
watch(outer, () => {
  void watch(inner, () => {}, { flush: 'sync' })
}, { flush: 'sync' })
</script>
<template>{{ outer }}{{ inner }}</template>
