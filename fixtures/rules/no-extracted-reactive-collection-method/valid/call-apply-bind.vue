<script setup lang="ts">
import { reactive } from 'vue'
const map = reactive(new Map([['a', 1]]))
const { get } = map
get.call(map, 'a')
get.apply(map, ['a'])
const bound = map.get.bind(map)
bound('a')
</script>

<template>
  <p />
</template>
