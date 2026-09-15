<script setup lang="ts">
import { ref, watch } from 'vue'
const count = ref(0)
watch(() => 1, () => {
  void count.value
})
</script>
<template>{{ count }}</template>
