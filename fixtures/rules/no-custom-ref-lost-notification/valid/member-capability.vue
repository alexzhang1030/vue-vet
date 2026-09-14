<script setup lang="ts">
import { customRef, watch } from 'vue'
const count = customRef((track, trigger) => {
  let value = 0
  const hooks = { track, trigger }
  return {
    get() {
      hooks.track()
      return value
    },
    set(next: number) {
      value = next
      hooks.trigger()
    },
  }
})
watch(count, () => {})
count.value = 1
</script>

<template>
  <p />
</template>
