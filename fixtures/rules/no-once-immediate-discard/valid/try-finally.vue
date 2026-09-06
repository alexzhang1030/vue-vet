<script setup lang="ts">
import { ref, watch } from 'vue'
const n = ref(0)
function accept(_value: unknown) {}
watch(
  n,
  (next, old) => {
    try {
      if (old === undefined) return
      accept(next)
    } finally {
      accept(next)
    }
  },
  { once: true, immediate: true },
)
</script>

<template>
  <p />
</template>
