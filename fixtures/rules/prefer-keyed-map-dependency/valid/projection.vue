<script setup lang="ts">
import { computed, reactive } from 'vue'
const keyed = reactive(new Map([['selected', { n: 1 }], ['other', { n: 2 }]]))
const selected = computed(() => {
  let value
  keyed.forEach((entry, key) => {
    if (key === 'selected') value = entry.n
  })
  return value
})
void selected
</script>

<template>
  <p />
</template>
