<script setup lang="ts">
import { reactive, watch } from 'vue'
const list = reactive([1])
function accept(_value: unknown) {}
watch(
  list,
  (next, old) => {
    if (old === undefined) return
    accept(next)
  },
  { once: true, immediate: true },
)
</script>

<template>
  <p />
</template>
