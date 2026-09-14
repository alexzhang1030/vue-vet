<script setup lang="ts">
import { computed, ref, watch } from 'vue'
const inner = ref(0)
const c0 = computed(() => 0)
const c1 = computed(() => c0.value)
const c2 = computed(() => c1.value)
const c3 = computed(() => c2.value)
watch(() => c3.value, () => {
  watch(inner, () => {}, { flush: 'sync' })
}, { immediate: true, flush: 'sync' })
</script>
<template>{{ inner }}</template>
