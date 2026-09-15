<script setup lang="ts">
import { reactive } from 'vue'
const raw = {}
const proxy = reactive(raw)
const map = new Map([[raw, { count: 1 }]])
const entry = map.get(proxy)
if (entry) void entry.count
</script>

<template>
  <p />
</template>
