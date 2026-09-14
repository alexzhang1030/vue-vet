<script setup lang="ts">
import { reactive } from 'vue'
const raw = {}
const proxy = reactive(raw)
const map = new Map([[raw, { count: 1 }]])
map.set(proxy, { count: 1 })
void map.get(proxy).count
</script>

<template>
  <p />
</template>
