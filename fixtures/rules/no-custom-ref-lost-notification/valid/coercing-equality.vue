<script setup lang="ts">
import { customRef, watch } from 'vue'
const count = customRef((track, trigger) => {
  let value: number | boolean = 0
  return {
    get() {
      track()
      return value
    },
    set(next: number | boolean) {
      if (value == next) return
      value = next
    },
  }
})
watch(count, () => {})
count.value = false
</script>

<template>
  <p />
</template>
