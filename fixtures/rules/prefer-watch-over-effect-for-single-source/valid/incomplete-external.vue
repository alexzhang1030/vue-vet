<script setup lang="ts">
import { ref, watchEffect } from 'vue'
const count = ref(0)
declare function external(value: number): number
const target = ref(0)
watchEffect(() => {
  target.value = external(count.value)
})
</script>
<template>{{ target }}</template>
