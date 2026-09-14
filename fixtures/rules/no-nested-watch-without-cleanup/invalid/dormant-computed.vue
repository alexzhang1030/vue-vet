<script setup lang="ts">
import { computed, ref, watch } from 'vue'
const outer = ref(0)
const inner = computed(() => 1)
watch(outer, () => {
  watch(inner, () => {}, { flush: 'sync' })
}, { flush: 'sync' })
</script>
<template>{{ outer }}{{ inner }}</template>
