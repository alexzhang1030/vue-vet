<script setup lang="ts">
import { computed, reactive } from 'vue'
const keyed = reactive(new Map([['selected', 7]]))
const selected = computed(() => {
  let value
  keyed.forEach(function* (entry, key) {
    if (key === 'selected') value = entry
  })
  return value
})
void selected
</script>

<template>
  <p />
</template>
