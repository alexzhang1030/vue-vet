<script setup lang="ts">
import { reactive, watchEffect } from 'vue'
const keyed = reactive(new Map([['selected', 1], ['other', 2]]))
watchEffect(() => {
  let value
  keyed.forEach((entry, key) => {
    if (key === 'selected') value = entry
  })
  void value
})
</script>

<template>
  <p />
</template>
