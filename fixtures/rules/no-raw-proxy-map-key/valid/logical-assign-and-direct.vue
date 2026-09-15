<script setup lang="ts">
import { reactive } from 'vue'
const raw = {}
const proxy = reactive(raw)
const map = new Map([[raw, { count: 7 }]])
let chosen = false
chosen &&= map.get(proxy).count
void chosen
</script>

<template>
  <p />
</template>
