<script setup lang="ts">
import { reactive } from 'vue'
const helper = new Map()
const api = helper
api.get = (target: object) => Object.preventExtensions(target)
const raw = {}
helper.get(raw)
const proxy = reactive(raw)
const map = new Map([[raw, { count: 7 }]])
void map.get(proxy).count
</script>

<template>
  <p />
</template>
