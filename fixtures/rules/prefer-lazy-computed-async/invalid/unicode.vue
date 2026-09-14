<script setup lang="ts">
import { ref, watch } from 'vue'
import { computedAsync } from '@vueuse/core'

const 源 = ref(1)
const 汇 = ref(0)
const 值 = computedAsync(async () => 源.value * 10, -1)
源.value = 3
watch(值, (current) => {
  if (current !== -1) 汇.value = current
}, { immediate: true })
</script>

<template>
  <p />
</template>
