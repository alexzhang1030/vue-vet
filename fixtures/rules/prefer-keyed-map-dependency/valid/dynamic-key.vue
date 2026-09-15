<script setup lang="ts">
import { computed, reactive } from 'vue'
const want = 'selected'
const keyed = reactive(new Map([['selected', 1], ['other', 2]]))
const selected = computed(() => {
  let value
  keyed.forEach((entry, key) => {
    if (key === want) value = entry
  })
  return value
})
void selected
</script>

<template>
  <p />
</template>
