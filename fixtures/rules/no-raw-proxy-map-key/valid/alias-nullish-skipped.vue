<script setup lang="ts">
import { reactive } from 'vue'
const raw = {}
const proxy = reactive(raw)
const map = new Map([[raw, { count: 1 }]])
const result = map.get(proxy)
0 ?? result.count
</script>

<template>
  <p />
</template>
