<script setup lang="ts">
import { watch } from 'vue'

declare function getUnknownBag(): { current: Set<string> }

const bag = getUnknownBag()
function readCurrent() {
  return bag.current
}

watch(() => {
  Promise.resolve().then(() => readCurrent())
  return 42
}, () => {})
</script>

<template>
  <p />
</template>
