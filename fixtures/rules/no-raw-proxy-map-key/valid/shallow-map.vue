<script setup lang="ts">
import { shallowReactive } from 'vue'
const raw = {}
const proxy = shallowReactive(raw)
const map = shallowReactive(new Map([[raw, { count: 1 }]]))
void map.get(proxy).count
</script>

<template>
  <p />
</template>
