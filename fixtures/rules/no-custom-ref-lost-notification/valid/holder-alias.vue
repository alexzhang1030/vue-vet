<script setup lang="ts">
import { customRef, watch } from 'vue'
const count = customRef((track, trigger) => {
  let value = 0
  const hooks = { track, trigger }
  const delegate = hooks
  return {
    get() {
      delegate.track()
      return value
    },
    set(next: number) {
      value = next
      delegate.trigger()
    },
  }
})
watch(count, () => {})
count.value = 1
</script>

<template>
  <p />
</template>
