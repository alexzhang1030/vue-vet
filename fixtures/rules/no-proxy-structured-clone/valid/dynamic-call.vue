<script setup lang="ts">
import { reactive } from 'vue'
const key = 'structuredClone'
globalThis[key](reactive({ count: 1 }))
</script>

<template>
  <p />
</template>
