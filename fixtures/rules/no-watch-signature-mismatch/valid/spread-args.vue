<script setup lang="ts">
import { ref, watch } from 'vue'
const count = ref(0)
const extra = [{ handler() {} }] as const
watch(count, ...extra)
</script>

<template>
  <p />
</template>
