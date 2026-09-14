<script setup lang="ts">
import { computed, reactive } from 'vue'
const token = {}
const keyed = reactive(new Map([[token, 1]]))
const selected = computed(() => {
  let value
  keyed.forEach((entry, key) => {
    if (key === token) value = entry
  })
  return value
})
void selected
</script>

<template>
  <p />
</template>
