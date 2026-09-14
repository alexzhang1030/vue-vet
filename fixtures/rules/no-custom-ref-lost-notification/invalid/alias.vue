<script setup lang="ts">
import { customRef as makeRef, watch as observe } from 'vue'
const count = makeRef((_track, trigger) => {
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
observe(count, () => {})
count.value = 1
</script>

<template>
  <p />
</template>
