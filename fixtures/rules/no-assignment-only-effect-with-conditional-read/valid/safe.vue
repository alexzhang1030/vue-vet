<script setup lang="ts">
import { ref, watch } from 'vue'
const enabled = ref(true)
const source = ref(1)
const target = ref(0)
watch([enabled, source], ([isEnabled, next]) => {
  if (isEnabled) {
    target.value = next
  }
})
</script>
<template>{{ target }}</template>
