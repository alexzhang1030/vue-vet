<script setup lang="ts">
const toRefs = (value: object) => value as { missing?: { value: number } }
const reactive = (value: object) => value
void toRefs(reactive({ count: 1 })).missing.value
</script>

<template>
  <p />
</template>
