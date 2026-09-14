<script setup lang="ts">
import { reactive } from 'vue'
const raw = {}
const proxy = reactive(raw)
const map = new Map([[raw, { count: 1 }]])
const answer = { undefined: 7 }
void answer[map.get(proxy) as unknown as string]
</script>

<template>
  <p />
</template>
