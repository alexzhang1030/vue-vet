<script setup lang="ts">
import { customRef, watch } from 'vue'
let notify: (() => void) | undefined
const count = customRef((track, trigger) => {
  let value = 0
  return {
    get() {
      track()
      return value
    },
    set(next: number) {
      value = next
      notify = trigger
    },
  }
})
watch(count, () => {})
count.value = 1
notify?.()
</script>

<template>
  <p />
</template>
