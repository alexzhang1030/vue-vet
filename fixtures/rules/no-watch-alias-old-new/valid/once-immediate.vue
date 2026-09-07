<script setup lang="ts">
import { reactive, watch } from 'vue'
const state = reactive({ n: 1 })
function accept(_value: unknown) {}
watch(
  state,
  (next, old) => {
    if (next === old) return
    accept(next)
  },
  { once: true, immediate: true },
)
</script>

<template>
  <p />
</template>
