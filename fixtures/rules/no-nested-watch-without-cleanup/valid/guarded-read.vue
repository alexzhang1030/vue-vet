<script setup lang="ts">
import { ref, watch, watchEffect } from 'vue'
const outer = ref(0)
const inner = ref(0)
watch(outer, () => {
  watchEffect(() => {
    if (outer.value) inner.value
  })
}, { flush: 'sync' })
</script>
<template>{{ outer }}{{ inner }}</template>
