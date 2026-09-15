<script setup lang="ts">
import { ref } from 'vue'
import { createSharedComposable } from '@vueuse/core'
const useValue = createSharedComposable((value: string | number) => ref(value))
function wrap(fn: typeof useValue) {
  const first = fn(1)
  const second = fn('text')
  void second.value.toUpperCase()
  return first
}
wrap(useValue)
</script>

<template>
  <p />
</template>
