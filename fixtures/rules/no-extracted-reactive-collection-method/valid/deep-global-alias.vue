<script setup lang="ts">
import { reactive } from 'vue'
function configure(world: typeof globalThis) {
  const previous = world.Map
  world.Map = class {
    get() {
      return 7
    }
  } as unknown as MapConstructor
  return () => {
    world.Map = previous
  }
}
const world = globalThis
const restore = configure(
  true &&
    (true && (true && (true && (true && (true && (true && (true && (true && world)))))))),
)
const items = reactive(new Map())
const { get } = items as { get: () => number }
get()
restore()
</script>

<template>
  <p />
</template>
