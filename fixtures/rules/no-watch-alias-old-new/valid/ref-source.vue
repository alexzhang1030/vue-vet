<script setup lang="ts">
import { ref, watch } from 'vue'
const n = ref(0)
function accept(_value: unknown) {}
watch(n, (next, old) => {
  if (next === old) return
  accept(next)
})
</script>

<template>
  <p />
</template>
