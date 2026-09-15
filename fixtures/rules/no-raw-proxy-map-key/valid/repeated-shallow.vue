<script setup lang="ts">
import { shallowReactive } from 'vue'
const raw = {}
const first = shallowReactive(raw)
const second = shallowReactive(raw)
const map = new Map([[first, { count: 7 }]])
void map.get(second).count
</script>

<template>
  <p />
</template>
