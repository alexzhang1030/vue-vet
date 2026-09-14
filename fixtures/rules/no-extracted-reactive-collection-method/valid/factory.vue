<script setup lang="ts">
import { reactive } from 'vue'
function createMap() {
  return new Map([['a', 1]])
}
const map = reactive(createMap())
const { get } = map
get('a')
</script>

<template>
  <p />
</template>
