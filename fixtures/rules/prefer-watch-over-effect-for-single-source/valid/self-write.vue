<script setup lang="ts">
import { ref, watchEffect } from 'vue'
const count = ref(0)
watchEffect(() => {
  count.value = count.value + 1
})
</script>
<template>{{ count }}</template>
