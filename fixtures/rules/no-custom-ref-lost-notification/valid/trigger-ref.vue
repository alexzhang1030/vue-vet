<script setup lang="ts">
import { customRef, triggerRef, watch } from 'vue'
const count = customRef((track, _trigger) => {
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
watch(count, () => {})
count.value = 1
triggerRef(count)
</script>

<template>
  <p />
</template>
