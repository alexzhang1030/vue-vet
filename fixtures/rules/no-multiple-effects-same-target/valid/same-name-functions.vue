<script setup>
import { ref, watchEffect } from 'vue'
function firstWriter() {
  const result = ref(0)
  const input = ref(1)
  watchEffect(() => {
    result.value = input.value
  })
  return result
}
function secondWriter() {
  const result = ref(0)
  const input = ref(2)
  watchEffect(() => {
    result.value = input.value
  })
  return result
}
const a = firstWriter()
const b = secondWriter()
</script>
<template>{{ a }} {{ b }}</template>
