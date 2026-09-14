<script setup lang="ts">
import { ref, watch } from 'vue'
const count = ref(0)
const extra = { flush: 'sync' as const }
watch(count, (n) => void n, { ...extra, equals: () => true })
</script>

<template>
  <p />
</template>
