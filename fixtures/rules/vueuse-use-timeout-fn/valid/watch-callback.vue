<script setup>
import { onMounted, ref, watch } from 'vue'
const value = ref(0)
watch(value, () => {
  setTimeout(() => {
    console.log('updated')
  }, 100)
})
onMounted(() => {
  console.log('mounted')
})
</script>
<template>{{ value }}</template>
