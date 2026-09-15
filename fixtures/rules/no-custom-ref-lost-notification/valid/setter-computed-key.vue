<script setup lang="ts">
import { customRef, watch } from 'vue'
const count = customRef((track, trigger) => {
  let value = 0
  return {
    get() {
      track()
      return value
    },
    set(next: number) {
      const result = { [next = 0]: true }
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
