<script setup>
import { ref, watchEffect } from 'vue'
const input = ref(0)
const other = ref(0)
const result = ref(0)
watchEffect(() => {
  result.value = input.value
})
watchEffect(() => {
  result.value = other.value
})
</script>
<template>{{ result }}</template>
