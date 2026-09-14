<script setup lang="ts">
import { reactive } from 'vue'
const undefined = 0
const raw = {}
const proxy = reactive(raw)
const map = new Map([[raw, { count: 1 }], [proxy, { count: 7 }]])
undefined ?? map.delete(proxy)
void map.get(proxy).count
</script>

<template>
  <p />
</template>
