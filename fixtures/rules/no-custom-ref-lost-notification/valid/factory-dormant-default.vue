<script setup lang="ts">
import { customRef, watch } from 'vue'
const count = customRef((track, trigger) => {
  let value = 0
  const { supplied = (value = 1) } = { supplied: true }
  return {
    get() {
      track()
      return value
    },
    set(next: number) {
      value = next
    },
  }
})
watch(count, () => {})
count.value = 0
</script>

<template>
  <p />
</template>
