<script setup lang="ts">
import { computed, reactive } from 'vue'
const keyed = reactive(new Map([['selected', 1], ['other', 2]]))
const selected = computed(() => {
  let value = 0
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
