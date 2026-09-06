<script setup lang="ts">
import { ref, watchEffect } from 'vue'
const source = ref(0)
declare const maybeCleanup: unknown
watchEffect(() => {
  source.value
  return maybeCleanup
})
</script>
<template>{{ source }}</template>
