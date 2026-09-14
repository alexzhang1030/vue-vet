<script setup lang="ts">
import { reactive } from 'vue'
const raw = [1] as { [key: string]: unknown; map?: () => number[] }
const marker = '__v_skip'
raw[marker] = true
raw.map = () => [7]
const items = reactive(raw)
const { map } = items
map()
</script>

<template>
  <p />
</template>
