<script setup lang="ts">
import { reactive } from 'vue'
function read(flag: boolean) {
  const raw = {}
  const proxy = reactive(raw)
  const map = new Map([[raw, { count: 1 }]])
  const result = map.get(proxy)
  if (flag) return 7
  void result.count
}
read(true)
</script>

<template>
  <p />
</template>
