<script setup lang="ts">
function reactive<T>(value: T): T {
  return value
}
const map = reactive(new Map([['a', 1]]))
const { get } = map
get('a')
</script>

<template>
  <p />
</template>
