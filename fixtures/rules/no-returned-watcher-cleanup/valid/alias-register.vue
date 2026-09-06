<script setup lang="ts">
import { ref, watchEffect } from 'vue'
const source = ref(0)
watchEffect((onCleanup) => {
  source.value
  const dispose = () => {}
  const alias = dispose
  onCleanup(alias)
  return dispose
})
</script>
<template>{{ source }}</template>
