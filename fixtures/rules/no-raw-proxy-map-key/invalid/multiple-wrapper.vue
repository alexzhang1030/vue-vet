<script setup lang="ts">
import { reactive, shallowReactive } from 'vue'
const raw = {}
const first = reactive(raw)
const second = shallowReactive(raw)
const map = new Map([[raw, { count: 1 }]])
void second
void map.get(first).count
</script>

<template>
  <p />
</template>
