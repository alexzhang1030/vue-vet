<script setup lang="ts">
import { reactive } from 'vue'
const raw = {}
const proxy = reactive(raw)
const stored = raw
const lookup = proxy
const map = new Map([[stored, { count: 1 }]])
void map.get(lookup).count
</script>

<template>
  <p />
</template>
