<script setup lang="ts">
function watch(_source: unknown, _cb: () => void, _opts?: object) {}
const n = { value: 0 }
function accept(_value: unknown) {}
watch(
  n,
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
