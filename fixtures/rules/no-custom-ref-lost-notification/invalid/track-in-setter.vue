<script setup lang="ts">
import { customRef, watch } from 'vue'
const count = customRef((track, trigger) => {
  let value = 0
  return {
    get() {
      return value
    },
    set(next: number) {
      track()
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
