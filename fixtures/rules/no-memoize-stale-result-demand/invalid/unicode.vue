<script setup lang="ts">
import { ref } from 'vue'
import { useMemoize } from '@vueuse/core'
const 源 = ref(1)
const 解析 = useMemoize(() => 源.value)
解析()
源.value = 'text'
解析().toUpperCase()
</script>

<template>
  <p />
</template>
