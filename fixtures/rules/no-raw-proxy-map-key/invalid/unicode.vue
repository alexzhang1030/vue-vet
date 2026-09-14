<script setup lang="ts">
import { reactive } from 'vue'
const 原 = {}
const 代理 = reactive(原)
const map = new Map([[原, { count: 1 }]])
void map.get(代理).count
</script>

<template>
  <p />
</template>
