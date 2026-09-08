<script setup lang="ts">
import { markRaw, reactive } from 'vue'
const raw = { count: 1 }
markRaw(raw)
const value = reactive(raw)
structuredClone(value)
</script>

<template>
  <p />
</template>
