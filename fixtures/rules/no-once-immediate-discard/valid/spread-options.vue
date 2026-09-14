<script setup lang="ts">
import { ref, watch } from 'vue'
const n = ref(0)
const extra = { flush: 'sync' as const }
function accept(_value: unknown) {}
watch(
  n,
  (next, old) => {
    if (old === undefined) return
    accept(next)
  },
  { once: true, immediate: true, ...extra },
)
</script>

<template>
  <p />
</template>
