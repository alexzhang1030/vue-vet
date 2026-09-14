<script setup lang="ts">
import { reactive } from 'vue'
globalThis['fetch'] = ((value: unknown) => value) as typeof fetch
structuredClone(reactive({ count: 1 }))
</script>

<template>
  <p />
</template>
