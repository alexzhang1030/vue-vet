<script setup lang="ts">
import { ref, watch } from 'vue'
const 外层 = ref(0)
const 内层 = ref(0)
watch(外层, () => {
  watch(内层, () => {}, { flush: 'sync' })
}, { flush: 'sync' })
</script>
<template>{{ 外层 }}{{ 内层 }}</template>
