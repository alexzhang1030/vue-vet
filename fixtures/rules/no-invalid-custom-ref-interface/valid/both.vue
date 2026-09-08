<script setup lang="ts">
import { customRef } from 'vue'
const count = customRef((track, trigger) => {
  let value = 0
  return {
    get() {
      track()
      return value
    },
    set(next: number) {
      value = next
      trigger()
    },
  }
})
count.value = 1
void count.value
</script>

<template>
  <p />
</template>
