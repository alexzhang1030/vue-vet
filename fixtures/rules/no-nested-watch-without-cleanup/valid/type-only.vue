<script setup lang="ts">
import { ref, watch, watchEffect } from 'vue'
const outer = ref(0)
const inner = ref(0)
watch(outer, () => {
  watchEffect(() => {
    type Source = typeof inner
    const unused: Source = inner
    void unused
  })
}, { flush: 'sync' })
</script>
<template>{{ outer }}{{ inner }}</template>
