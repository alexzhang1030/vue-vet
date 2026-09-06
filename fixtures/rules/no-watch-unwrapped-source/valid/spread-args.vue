<script setup lang="ts">
import { ref, watch } from 'vue'
const n = ref(0)
const extra: unknown[] = []
watch(n.value, ...extra)
</script>

<template>
  <p />
</template>
