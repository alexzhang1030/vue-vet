<script setup lang="ts">
import { ref, watch } from 'vue'
const outer = ref(0)
const inner = ref(0)
watch(outer, () => {
  watch(() => { return 0; inner.value }, () => {}, { flush: 'sync' })
}, { flush: 'sync' })
</script>
<template>{{ outer }}{{ inner }}</template>
