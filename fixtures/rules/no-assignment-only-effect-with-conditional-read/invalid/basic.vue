<script setup lang="ts">
import { ref, watchEffect } from 'vue'
const enabled = ref(true)
const source = ref(1)
const target = ref(0)
watchEffect(() => {
  target.value = enabled.value ? source.value : 0
})
</script>
<template>{{ target }}</template>
