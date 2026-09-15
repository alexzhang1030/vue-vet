<script setup lang="ts">
import * as Vue from 'vue'
const count = Vue.customRef((_track, trigger) => {
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
Vue.watch(count, () => {})
count.value = 1
</script>

<template>
  <p />
</template>
