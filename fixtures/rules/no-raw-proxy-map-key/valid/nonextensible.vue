<script setup lang="ts">
import { reactive } from 'vue'
const raw = {}
Object.preventExtensions(raw)
const proxy = reactive(raw)
const map = new Map([[raw, { count: 7 }]])
void map.get(proxy).count
</script>

<template>
  <p />
</template>
