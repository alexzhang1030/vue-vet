<script setup lang="ts">
import { reactive } from 'vue'
const original = Map
try {
  ;(globalThis as { Map: typeof Map }).Map = class {
    get() {
      return 7
    }
  } as unknown as MapConstructor
  const items = reactive(new Map())
  const { get } = items as { get: () => number }
  get()
} finally {
  ;(globalThis as { Map: typeof Map }).Map = original
}
</script>

<template>
  <p />
</template>
