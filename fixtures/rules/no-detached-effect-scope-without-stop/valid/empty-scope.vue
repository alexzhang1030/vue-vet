<script setup lang="ts">
import { effectScope, ref, watch } from 'vue'
const outer = ref(0)
watch(outer, () => {
  const scope = effectScope(true)
  scope.run(() => {})
}, { flush: 'sync' })
</script>
<template>{{ outer }}</template>
