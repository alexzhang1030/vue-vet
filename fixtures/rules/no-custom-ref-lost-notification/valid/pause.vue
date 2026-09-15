<script setup lang="ts">
import { customRef, watchEffect } from 'vue'
const count = customRef((_track, trigger) => {
  let value = 0
  return {
    get() {
      return value
    },
    set(next: number) {
      value = next
      trigger()
    },
  }
})
const handle = watchEffect(() => {
  void count.value
})
handle.pause()
count.value = 1
</script>

<template>
  <p />
</template>
