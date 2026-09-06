<script setup lang="ts">
function toRefs(_value: object) { return {} }
const { a } = toRefs({ a: 1 })
void a
</script>

<template>
  <p />
</template>
