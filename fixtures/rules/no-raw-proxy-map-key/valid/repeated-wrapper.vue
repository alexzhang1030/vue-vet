<script setup lang="ts">
import { reactive } from 'vue'
const raw = {}
const first = reactive(raw)
const second = reactive(raw)
const map = new Map([[raw, { count: 1 }], [first, { count: 7 }]])
void map.get(second).count
</script>

<template>
  <p />
</template>
