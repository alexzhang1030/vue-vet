<script setup lang="ts">
import { ref, watchEffect } from 'vue'
const source = ref(0)
watchEffect(() => {
  source.value
  return () => {}
})
</script>
<template>{{ source }}</template>
