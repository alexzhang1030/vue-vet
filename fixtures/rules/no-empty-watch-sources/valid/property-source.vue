<script setup lang="ts">
import { watch } from 'vue'

declare function createUnknownState(): { current: Set<string> }

const state = createUnknownState()
watch(() => state.current, () => {}, { deep: true })
</script>

<template>
  <p />
</template>
