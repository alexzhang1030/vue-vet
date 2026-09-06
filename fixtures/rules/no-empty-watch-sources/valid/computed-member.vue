<script setup lang="ts">
import { watch } from 'vue'

declare function createUnknownSources(): Record<string, { value: boolean }>

const sources = createUnknownSources()
watch(sources['active'], (active) => {
  if (active)
    return
})
</script>

<template>
  <p />
</template>
