<script setup lang="ts">
import { ref, watch } from 'vue'
const n = ref(0)
function accept(_value: unknown) {}
watch(
  n,
  (next, ...rest) => {
    const old = rest[0]
    if (old === undefined) return
    accept(next)
  },
  { once: true, immediate: true },
)
</script>

<template>
  <p />
</template>
