<script setup lang="ts">
import { ref, watch } from 'vue'
const source = ref(0)
watch(
  () => {
    source.value
    return () => source.value
  },
  () => {},
)
</script>
<template>{{ source }}</template>
