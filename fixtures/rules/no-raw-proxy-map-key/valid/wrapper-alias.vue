<script setup lang="ts">
import { reactive } from 'vue'
const raw = {}
const first = reactive(raw)
const alias = first
const map = new Map([[raw, { count: 1 }], [first, { count: 7 }]])
void map.get(alias).count
</script>

<template>
  <p />
</template>
