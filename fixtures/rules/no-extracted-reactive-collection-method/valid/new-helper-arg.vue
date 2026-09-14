<script setup lang="ts">
import { reactive } from 'vue'
class Box {
  constructor(public value: object) {}
}
const map = reactive(new Map([['a', 1]]))
void new Box(map)
const { get } = map
get('a')
</script>

<template>
  <p />
</template>
