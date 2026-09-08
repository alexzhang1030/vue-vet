<script setup lang="ts">
import { reactive } from 'vue'
for (globalThis.structuredClone in { x: 1 }) {}
structuredClone(reactive({ count: 1 }))
</script>

<template>
  <p />
</template>
