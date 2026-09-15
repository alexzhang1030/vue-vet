<script setup lang="ts">
import { computed, ref, watch } from 'vue'
const inner = ref(0)
const left = computed(() => right.value)
const right = computed(() => left.value)
watch(left, () => {
  watch(inner, () => {}, { flush: 'sync' })
}, { flush: 'sync' })
</script>
<template>{{ inner }}</template>
