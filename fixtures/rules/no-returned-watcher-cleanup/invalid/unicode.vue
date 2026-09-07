<script setup lang="ts">
import { ref, watchEffect } from 'vue'
const 源 = ref(0)
watchEffect(() => {
  源.value
  const 清理 = () => {}
  return 清理
})
</script>
<template>{{ 源 }}</template>
