<script setup lang="ts">
import { ref } from 'vue'
import { useMemoize as memo } from '@vueuse/core'
const source = ref(1)
const resolve = memo(() => source.value)
resolve()
source.value = 'text'
resolve().toUpperCase()
</script>

<template>
  <p />
</template>
