<script setup lang="ts">
import { ref, watch, watchEffect } from 'vue'
const outer = ref(0)
const inner = ref(0)
let snapshot = 0
watch(outer, () => {
  watchEffect(() => {
    ({ value: snapshot = inner.value } = {})
  }, { flush: 'sync' })
}, { flush: 'sync' })
</script>
<template>{{ outer }}{{ inner }}{{ snapshot }}</template>
