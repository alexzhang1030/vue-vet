<script setup lang="ts">
import { reactive } from 'vue'
const map = reactive(new Map([['a', 1]]))
map.get('a')
map.set('b', 2)
map.has('a')
const items = reactive(new Set([1]))
items.has(1)
items.add(2)
const list = reactive([1, 2])
list.map((n) => n)
list.includes(1)
list.push(3)
</script>

<template>
  <p />
</template>
