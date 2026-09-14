<script setup lang="ts">
import { reactive } from 'vue'
const raw = {}
const proxy = reactive(raw)
const extra = ((value: object) => value)(proxy)
const map = new Map([[raw, { count: 1 }], [extra, { count: 7 }]])
void map.get(proxy).count
</script>

<template>
  <p />
</template>
