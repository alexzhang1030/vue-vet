<script setup lang="ts">
import * as Vue from 'vue'
const keyed = Vue.reactive(new Map([['selected', 1], ['other', 2]]))
const selected = Vue.computed(() => {
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
