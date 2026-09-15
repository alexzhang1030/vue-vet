<script setup lang="ts">
import { reactive } from 'vue'
const raw = { __v_skip: true }
const proxy = reactive(raw)
const map = new Map([[raw, { count: 7 }]])
void map.get(proxy).count
</script>

<template>
  <p />
</template>
