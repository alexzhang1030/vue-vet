<script setup lang="ts">
import { reactive } from 'vue'
const raw = {}
const proxy = reactive(raw)
class MapLike { get(_key: object) { return { count: 1 } } }
const map = new MapLike()
void map.get(proxy).count
</script>

<template>
  <p />
</template>
