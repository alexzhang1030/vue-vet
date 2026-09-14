<script setup lang="ts">
import { customRef, watchEffect, watchPostEffect } from 'vue'
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
watchPostEffect(() => {
  void count.value
})
watchEffect(
  () => {
    void count.value
  },
  { flush: 'post' },
)
count.value = 1
</script>

<template>
  <p />
</template>
