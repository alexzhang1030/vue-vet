<script setup lang="ts">
import { ref, watch, watchEffect } from 'vue'
const outer = ref(0)
const inner = ref(0)
let snapshot
watch(outer, () => {
  watchEffect(() => {
    ({ [inner.value]: snapshot } = {})
  }, { flush: 'sync' })
}, { flush: 'sync' })
</script>
<template>{{ outer }}{{ inner }}</template>
