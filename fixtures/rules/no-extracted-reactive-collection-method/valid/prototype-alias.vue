<script setup lang="ts">
import { reactive } from 'vue'
function configure(prototype: typeof Array.prototype) {
  const previous = prototype.map
  prototype.__v_skip = true
  prototype.map = () => [7]
  return () => {
    delete prototype.__v_skip
    prototype.map = previous
  }
}
const capability = Array.prototype
const restore = configure(capability)
const items = reactive([1])
const { map } = items
map()
restore()
</script>

<template>
  <p />
</template>
