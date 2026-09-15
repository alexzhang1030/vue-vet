<script setup lang="ts">
import { reactive } from 'vue'
const raw = {}
const proxy = reactive(raw)
const map = new Map([[proxy, { count: 1 }]])
void map.get(raw).count
</script>

<template>
  <p />
</template>
