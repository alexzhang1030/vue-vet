<script setup lang="ts">
import { shallowReactive, watch } from 'vue'
const state = shallowReactive({ n: 1 })
function accept(_value: unknown) {}
watch(state, (next, old) => {
  if (next === old) return
  accept(next)
})
</script>

<template>
  <p />
</template>
