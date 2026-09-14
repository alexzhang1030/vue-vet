<script setup lang="ts">
import { ref, watch } from 'vue'
const 源 = ref('one')
const 结果 = ref<string | null>(null)
watch(源, async (值, _previous, onCleanup) => {
  const 数据 = await Promise.resolve(值)
  let 已取消 = false
  onCleanup(() => {
    已取消 = true
  })
  if (!已取消) 结果.value = 数据
})
</script>
<template>{{ 结果 }}</template>
