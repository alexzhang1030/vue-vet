<script setup lang="ts">
import { customRef, watch } from 'vue'
const count = customRef((track, trigger) => {
  let value = 0
  return {
    get() {
      ;(() => track())()
      return value
    },
    set(next: number) {
      value = next
      trigger()
    },
  }
})
watch(count, () => {})
count.value = 1
</script>

<template>
  <p />
</template>
