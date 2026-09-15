<script setup lang="ts">
import { customRef } from 'vue'
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
void count.value
count.value = 1
</script>

<template>
  <p />
</template>
