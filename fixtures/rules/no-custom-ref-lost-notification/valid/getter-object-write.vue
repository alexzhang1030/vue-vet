<script setup lang="ts">
import { customRef, watch } from 'vue'
const count = customRef((track, trigger) => {
  let value = 0
  return {
    get() {
      const result = { value: (value = 0) }
      track()
      return value
    },
    set(next: number) {
      value = next
    },
  }
})
watch(count, () => {})
count.value = 1
</script>

<template>
  <p />
</template>
