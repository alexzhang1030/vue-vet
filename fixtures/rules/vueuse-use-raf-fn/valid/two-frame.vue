<script setup>
import { onMounted, ref } from 'vue'
const ready = ref(false)
function render() { ready.value = true }
onMounted(() => {
  requestAnimationFrame(() => { requestAnimationFrame(render) })
})
</script>
<template>{{ ready }}</template>
