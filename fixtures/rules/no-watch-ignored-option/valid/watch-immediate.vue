<script setup lang="ts">
import { ref, watch } from 'vue'
const count = ref(0)
watch(count, (n) => void n, { immediate: true, deep: true, once: true })
</script>

<template>
  <p />
</template>
