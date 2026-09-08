<script setup lang="ts">
import { readonly } from 'vue'
const raw = { count: 1 }
Object.freeze(raw)
structuredClone(readonly(raw))
</script>

<template>
  <p />
</template>
