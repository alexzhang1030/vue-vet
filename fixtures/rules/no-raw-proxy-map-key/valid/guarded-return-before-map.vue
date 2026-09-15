<script setup lang="ts">
import { reactive } from 'vue'
function read(flag: boolean) {
  if (flag) return 7
  const raw = {}
  const proxy = reactive(raw)
  const map = new Map([[raw, { count: 1 }]])
  void map.get(proxy).count
}
read(true)
</script>

<template>
  <p />
</template>
