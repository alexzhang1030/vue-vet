<script setup lang="ts">
import { ref, watch } from 'vue'
const source = ref({ n: 1 })
function accept(_value: unknown) {}
watch(
  source,
  (next, old) => {
    if (next === old) return
    accept(next)
  },
  { deep: true },
)
</script>

<template>
  <p />
</template>
