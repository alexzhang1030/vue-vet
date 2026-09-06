<script setup lang="ts">
import { watch } from 'vue'

declare function getUnknownBag(): { current: Set<string> }

const bag = getUnknownBag()
function readCurrent() {
  return bag.current
}

watch(() => readCurrent(), () => {}, { deep: true })
</script>

<template>
  <p />
</template>
