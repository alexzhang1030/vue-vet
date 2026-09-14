<script setup lang="ts">
import { ref, watch } from 'vue'
const outer = ref(0)
const inner = ref(0)
watch(outer, (_value, _old, onCleanup) => {
  const stop = watch(inner, () => {}, { flush: 'sync' })
  onCleanup(() => stop())
}, { flush: 'sync' })
</script>
<template>{{ outer }}{{ inner }}</template>
