<script setup lang="ts">
import { reactive } from 'vue'
const raw = {}
const proxy = reactive(raw)
declare const map: Map<object, { count: number }>
void map.get(proxy).count
</script>

<template>
  <p />
</template>
