<script setup lang="ts">
import { computed, ref, watch } from 'vue'

const items = ref([1, 2])
const doubled = computed((previous: number[] | undefined) => {
  const next = items.value.map((n) => n * 2)
  if (
    previous
    && previous.length === next.length
    && previous.every((value, index) => Object.is(value, next[index]))
  ) {
    return previous
  }
  return next
})
watch(doubled, (value) => {
  void value
})
items.value = [1, 2]
</script>

<template>
  <p />
</template>
