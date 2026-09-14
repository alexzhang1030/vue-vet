<script setup lang="ts">
import { reactive, watch } from 'vue'
const frozenTarget = { n: 0 }
Object.freeze(frozenTarget)
const frozenState = reactive(frozenTarget)
function accept(_value: unknown) {}
watch(frozenState, (next, old) => {
  if (next === old) return
  accept(next)
})
</script>

<template>
  <p />
</template>
