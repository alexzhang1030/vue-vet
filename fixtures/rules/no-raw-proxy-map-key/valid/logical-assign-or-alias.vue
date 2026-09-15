<script setup lang="ts">
import { reactive } from 'vue'
const raw = {}
const proxy = reactive(raw)
const map = new Map([[raw, { count: 7 }]])
const result = map.get(proxy)
let chosen = true
chosen ||= result.count
void chosen
</script>

<template>
  <p />
</template>
