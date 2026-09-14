<script setup lang="ts">
import { reactive } from 'vue'
const raw = {}
const first = reactive(raw)
const map = new Map([[first, { count: 7 }]])
void map.get(reactive(raw)).count
</script>

<template>
  <p />
</template>
