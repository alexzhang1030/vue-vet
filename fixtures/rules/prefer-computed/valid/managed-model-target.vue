<script setup>
import { ref, watchEffect } from 'vue'
const target = defineModel()
const source = ref(1)
watchEffect(() => {
  target.value = source.value
})
</script>
<template>{{ target }}</template>
