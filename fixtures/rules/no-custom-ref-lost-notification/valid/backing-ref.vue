<script setup lang="ts">
import { customRef, ref, watch } from 'vue'
const backing = ref(0)
const count = customRef(() => ({
  get() {
    return backing.value
  },
  set(next: number) {
    backing.value = next
  },
}))
watch(count, () => {})
count.value = 1
</script>

<template>
  <p />
</template>
