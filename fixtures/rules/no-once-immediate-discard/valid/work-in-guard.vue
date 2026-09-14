<script setup lang="ts">
import { ref, watch } from 'vue'
const n = ref(0)
function accept(_value: unknown) {}
watch(
  n,
  (next, old) => {
    if (old === undefined) {
      accept(next)
      return
    }
    accept(next)
  },
  { once: true, immediate: true },
)
</script>

<template>
  <p />
</template>
