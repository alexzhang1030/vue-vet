<script setup lang="ts">
import { customRef, watch } from 'vue'
const count = customRef((track, trigger) => {
  let value = 0
  const undefined = 7
  const initialized = undefined ?? (value = 1)
  void initialized
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
