<script setup lang="ts">
import { reactive } from 'vue'
const raw = {}
const proxy = reactive(raw)
function Map() { return new WeakMap() }
const map = new Map([[raw, { count: 1 }]])
void map.get(proxy).count
</script>

<template>
  <p />
</template>
