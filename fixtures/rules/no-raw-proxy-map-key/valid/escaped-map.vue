<script setup lang="ts">
import { reactive } from 'vue'
const raw = {}
const proxy = reactive(raw)
const map = new Map([[raw, { count: 1 }]])
retain(map)
void map.get(proxy).count
function retain(value: unknown) { void value }
</script>

<template>
  <p />
</template>
