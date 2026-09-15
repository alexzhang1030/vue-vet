<script setup lang="ts">
import { reactive } from 'vue'
function read() {
  return 7
  const raw = {}
  const proxy = reactive(raw)
  const map = new Map([[raw, { count: 1 }]])
  void map.get(proxy).count
}
read()
</script>

<template>
  <p />
</template>
