<script setup lang="ts">
import { reactive } from 'vue'
Map.prototype.get = function get(this: Map<unknown, unknown>, key: unknown) {
  return Map.prototype.has.call(this, key)
}
const map = reactive(new Map([['a', 1]]))
const { get } = map
get('a')
</script>

<template>
  <p />
</template>
