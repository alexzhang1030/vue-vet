<script setup lang="ts">
import { reactive } from 'vue'
;(globalThis.structuredClone as typeof structuredClone) = ((value: unknown) =>
  value) as typeof structuredClone
structuredClone(reactive({ count: 1 }))
</script>

<template>
  <p />
</template>
