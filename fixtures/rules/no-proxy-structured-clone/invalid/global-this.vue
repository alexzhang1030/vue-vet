<script setup lang="ts">
import { reactive } from 'vue'
globalThis.structuredClone(reactive({ count: 1 }))
</script>

<template>
  <p />
</template>
