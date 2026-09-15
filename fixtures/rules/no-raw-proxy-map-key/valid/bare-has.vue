<script setup lang="ts">
import { reactive } from 'vue'
const raw = {}
const proxy = reactive(raw)
const map = new Map([[raw, { count: 1 }]])
void map.has(proxy)
</script>

<template>
  <p />
</template>
