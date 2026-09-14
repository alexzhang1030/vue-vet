<script setup lang="ts">
import { ref, watch, watchEffect } from 'vue'
const inner = ref(0)
watchEffect(() => {
  watch(inner, () => {}, { flush: 'sync' })
})
</script>
<template>{{ inner }}</template>
