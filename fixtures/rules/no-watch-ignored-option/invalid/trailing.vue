<script setup lang="ts">
import { ref, watch } from 'vue'
const count = ref(0)
watch(count, (n) => void n, { equals: () => true }, 'extra')
</script>

<template>
  <p />
</template>
