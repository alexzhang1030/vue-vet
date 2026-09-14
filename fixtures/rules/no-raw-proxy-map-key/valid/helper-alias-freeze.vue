<script setup lang="ts">
import { reactive } from 'vue'
const helper = { get(target: object) { Object.preventExtensions(target) } }
const api = helper
const raw = {}
api.get(raw)
const proxy = reactive(raw)
const map = new Map([[raw, { count: 7 }]])
void map.get(proxy).count
</script>

<template>
  <p />
</template>
