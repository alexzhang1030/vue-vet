<script setup lang="ts">
import { reactive } from 'vue'
function read(): never {
  const raw = {}
  const proxy = reactive(raw)
  const map = new Map([[raw, { count: 1 }]])
  const result = map.get(proxy)
  throw 7
  void result.count
}
</script>

<template>
  <p />
</template>
