<script setup lang="ts">
import { customRef, watch } from 'vue'
function delegate(track: () => void, trigger: () => void) {
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
}
const count = customRef((track, trigger) => delegate(track, trigger))
watch(count, () => {})
count.value = 1
</script>

<template>
  <p />
</template>
