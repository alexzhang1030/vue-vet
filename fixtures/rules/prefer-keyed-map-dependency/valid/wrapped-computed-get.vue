<script setup lang="ts">
import { computed, reactive } from 'vue'
const inner = new Map([['selected', 7]])
inner['get'] = () => 9
const keyed = reactive(inner)
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
