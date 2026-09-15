<script setup lang="ts">
import { ref, watch, watchEffect } from 'vue'
const outer = ref(0)
const inner = ref(0)
watch(outer, () => {
  watchEffect(() => {
    inner.value = 1
  })
}, { flush: 'sync' })
</script>
<template>{{ outer }}{{ inner }}</template>
