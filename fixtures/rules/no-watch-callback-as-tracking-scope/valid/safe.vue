<script setup lang="ts">
import { ref, watch } from 'vue'
const count = ref(0)
watch(count, () => {
  console.log('changed')
})
</script>
<template>{{ count }}</template>
