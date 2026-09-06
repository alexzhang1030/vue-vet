<script setup lang="ts">
import { ref, watchEffect } from 'vue'
const source = ref(0)
function attach(onCleanup: (fn: () => void) => void) {
  source.value
  const dispose = () => {}
  onCleanup(dispose)
  return dispose
}
watchEffect(attach)
</script>
<template>{{ source }}</template>
