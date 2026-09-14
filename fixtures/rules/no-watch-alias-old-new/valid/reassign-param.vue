<script setup lang="ts">
import { reactive, watch } from 'vue'
const state = reactive({ n: 1 })
function accept(_value: unknown) {}
watch(state, (next, old) => {
  old = { n: 0 }
  if (next === old) return
  accept(next)
})
</script>

<template>
  <p />
</template>
