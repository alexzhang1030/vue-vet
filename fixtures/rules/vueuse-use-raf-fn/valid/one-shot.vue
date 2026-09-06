<script setup>
import { onMounted, ref } from 'vue'
const ready = ref(false)
onMounted(() => {
  requestAnimationFrame(() => {
    ready.value = true
  })
})
</script>
<template>{{ ready }}</template>
