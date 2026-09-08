<script setup lang="ts">
import { reactive, watch } from 'vue'
const frozen = { n: 0 }
Object.freeze(...[frozen])
const state = reactive(frozen)
function accept(_value: unknown) {}
watch(state, (next, old) => {
  if (next === old) return
  accept(next)
})
</script>

<template>
  <p />
</template>
