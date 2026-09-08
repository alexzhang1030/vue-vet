<script setup lang="ts">
import { reactive } from 'vue'
const key = 'structuredClone'
globalThis[key] = ((value: unknown) => value) as typeof structuredClone
structuredClone(reactive({ count: 1 }))
</script>

<template>
  <p />
</template>
