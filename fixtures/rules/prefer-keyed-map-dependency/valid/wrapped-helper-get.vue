<script setup lang="ts">
import { computed, reactive } from 'vue'
function alter(target: Map<string, number>) {
  target.get = () => 9
}
const inner = new Map([['selected', 7]])
alter(inner)
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
