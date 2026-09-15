<script setup lang="ts">
import { reactive } from 'vue'
const raw = {}
const proxy = reactive(raw)
const map = new Map([[raw, { count: 1 }]])
function read() {
  return map.get(proxy).count
}
map.set(proxy, { count: 7 })
void read()
</script>

<template>
  <p />
</template>
