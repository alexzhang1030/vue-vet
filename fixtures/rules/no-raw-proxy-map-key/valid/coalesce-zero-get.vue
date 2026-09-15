<script setup lang="ts">
import { reactive } from 'vue'
const raw = {}
const proxy = reactive(raw)
const map = new Map([[raw, { count: 1 }]])
void (0 ?? map.get(proxy).count)
</script>

<template>
  <p />
</template>
