<script setup lang="ts">
import { ref, watch } from 'vue'
const 处理 = () => {}
const 源 = ref(new EventTarget())
watch(源, (目标, _prev, onCleanup) => {
  目标.addEventListener('click', 处理)
  onCleanup(() => {
    源.value.removeEventListener('click', 处理)
  })
}, { immediate: true, flush: 'sync' })
源.value = new EventTarget()
</script>
<template>{{ 源 }}</template>
