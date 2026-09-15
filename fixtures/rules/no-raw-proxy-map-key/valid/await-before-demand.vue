<script setup lang="ts">
import { reactive } from 'vue'
async function read() {
  const raw = {}
  const proxy = reactive(raw)
  const map = new Map([[raw, { count: 1 }]])
  const result = map.get(proxy)
  await Promise.resolve()
  void result.count
}
</script>

<template>
  <p />
</template>
