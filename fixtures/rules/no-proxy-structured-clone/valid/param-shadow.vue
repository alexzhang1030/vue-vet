<script setup lang="ts">
import { reactive } from 'vue'
function wrap(structuredClone: (value: unknown) => unknown) {
  structuredClone(reactive({ count: 1 }))
}
void wrap
</script>

<template>
  <p />
</template>
