<script setup lang="ts">
import { reactive } from 'vue'
const object = reactive({
  get: () => 1,
  add: () => 2,
  map: () => 3,
})
const { get, add, map } = object
get()
add()
map()
</script>

<template>
  <p />
</template>
