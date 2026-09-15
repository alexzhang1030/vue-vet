<script setup lang="ts">
import { customRef, ref, watch } from 'vue'
const backing = ref(0)
const count = customRef((_track, _trigger) => {
  let value = 0
  return {
    get() {
      return value
    },
    set(next: number) {
      value = next
    },
  }
})
count._get = () => backing.value
count._set = (next: number) => {
  backing.value = next
}
watch(count, () => {})
count.value = 1
</script>

<template>
  <p />
</template>
