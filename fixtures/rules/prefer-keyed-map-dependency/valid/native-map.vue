<script setup lang="ts">
import { computed } from 'vue'
const keyed = new Map([['selected', 1], ['other', 2]])
const selected = computed(() => {
  let value
  keyed.forEach((entry, key) => {
    if (key === 'selected') value = entry
  })
  return value
})
void selected
</script>

<template>
  <p />
</template>
