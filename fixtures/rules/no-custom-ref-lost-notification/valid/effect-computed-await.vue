<script setup lang="ts">
import { customRef, watchEffect } from 'vue'
const count = customRef((track, trigger) => {
  let value = 0
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
watchEffect(async () => {
  const resolved = { [await Promise.resolve()]: true }
  void resolved
  void count.value
})
count.value = 1
</script>

<template>
  <p />
</template>
