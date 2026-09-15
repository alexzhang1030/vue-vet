<script setup lang="ts">
import { effectScope, ref, watch } from 'vue'
const 外层 = ref(0)
const 内层 = ref(0)
watch(外层, () => {
  const 作用域 = effectScope(true)
  作用域.run(() => {
    watch(内层, () => {}, { flush: 'sync' })
  })
}, { flush: 'sync' })
</script>
<template>{{ 外层 }}{{ 内层 }}</template>
