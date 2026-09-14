<script setup lang="ts">
import { reactive } from 'vue'
const raw: { n?: number } = {}
raw.n = 1
const proxy = reactive(raw)
const map = new Map([[raw, { count: 1 }]])
void map.get(proxy).count
</script>

<template>
  <p />
</template>
