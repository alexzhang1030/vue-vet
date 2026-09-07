<script setup lang="ts">
import { ref, watchEffect } from 'vue'
const source = ref(0)
watchEffect((onCleanup) => {
  source.value
  onCleanup(() => {})
})
</script>
<template>{{ source }}</template>
