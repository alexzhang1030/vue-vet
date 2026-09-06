<script setup>
import { ref, watchEffect } from 'vue'
const input = ref(false)
const result = ref(false)
watchEffect(() => {
  if (input.value) {
    result.value = true
    return
  }
  result.value = false
})
</script>
<template>{{ result }}</template>
