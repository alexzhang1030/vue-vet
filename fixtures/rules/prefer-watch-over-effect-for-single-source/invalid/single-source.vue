<script setup lang="ts">
import { ref, watchEffect } from 'vue'
const count = ref(0)
const target = ref(0)
watchEffect(() => {
  target.value = count.value
})
</script>
<template>{{ target }}</template>
