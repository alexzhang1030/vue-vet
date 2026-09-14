<script setup lang="ts">
function watch(_source: unknown, _cb: () => void) {}
const state = { n: 1 }
function accept(_value: unknown) {}
watch(state, (next, old) => {
  if (next === old) return
  accept(next)
})
</script>

<template>
  <p />
</template>
