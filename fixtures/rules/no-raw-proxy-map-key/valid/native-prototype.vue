<script setup lang="ts">
import { reactive } from 'vue'
const original = Map.prototype.get
Map.prototype.get = function get(this: Map<unknown, unknown>, key: unknown) {
  return original.call(this, key)
}
const raw = {}
const proxy = reactive(raw)
const map = new Map([[raw, { count: 1 }]])
void map.get(proxy).count
</script>

<template>
  <p />
</template>
