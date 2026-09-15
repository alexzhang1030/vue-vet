<script setup lang="ts">
import { reactive } from 'vue'
const raw = {}
const proxy = reactive(raw)
const map = new Map([[raw, { count: 1 }]])
function tag(_strings: TemplateStringsArray, value: unknown) {
  return value
}
tag`${map.get(proxy).count}`
</script>

<template>
  <p />
</template>
