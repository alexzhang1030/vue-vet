<script setup lang="ts">
import { customRef, watch } from 'vue'
const count = customRef((track, trigger) => {
  let value = 0
  return {
    get() {
      const result = { Reset: class { static { value = 0 } } }
      void result
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
