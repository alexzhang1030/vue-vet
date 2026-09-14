<script setup lang="ts">
import { customRef, watch } from 'vue'
const factory = (track: () => void, trigger: () => void) => {
  let value = 0
  return {
    get() {
      return value
    },
    set(next: number) {
      value = next
    },
  }
}
const count = customRef(factory)
watch(count, () => {})
count.value = 1
</script>

<template>
  <p />
</template>
