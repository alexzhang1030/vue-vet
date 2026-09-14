<script setup lang="ts">
import { computed, ref, watch } from 'vue'
const inner = ref(0)
const looped = computed(() => looped.value)
watch(looped, () => {
  watch(inner, () => {}, { flush: 'sync' })
}, { flush: 'sync' })
</script>
<template>{{ inner }}</template>
