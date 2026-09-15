<script setup lang="ts">
import { reactive } from 'vue'
const raw = {}
const proxy = reactive(raw)
const map = new Map([[raw, { count: 7 }]])
const result = map.get(proxy)
const sink = null
void sink?.field[result.count]
</script>

<template>
  <p />
</template>
