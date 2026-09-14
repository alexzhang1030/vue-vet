<script setup lang="ts">
import { customRef, watch } from 'vue'
const 状态 = customRef((_track, trigger) => {
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
watch(状态, () => {})
状态.value = 1
</script>

<template>
  <p />
</template>
