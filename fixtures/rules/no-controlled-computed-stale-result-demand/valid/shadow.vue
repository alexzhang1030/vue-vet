<script setup lang="ts">
const computedWithControl = (_source: unknown, getter: () => unknown) => ({
  value: getter(),
  trigger() {},
})
const revision = { value: 0 }
const source = { value: 1 }
const value = computedWithControl(revision, () => source.value)
void value.value
source.value = 'text'
value.value.toUpperCase()
</script>

<template>
  <p />
</template>
