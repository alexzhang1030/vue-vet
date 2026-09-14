<script setup lang="ts">
function toRef(_source: unknown, _key: string) {
  return { value: 0 }
}
const count = { value: 0 }
const aliased = toRef(count, 'n')
void aliased
</script>

<template>
  <p />
</template>
