<script setup lang="ts">
function unref(value: unknown) {
  return value
}

const n = unref(1)
void n
</script>

<template>
  <div />
</template>
