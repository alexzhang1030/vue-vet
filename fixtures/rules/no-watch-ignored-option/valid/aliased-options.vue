<script setup lang="ts">
import { ref, watch } from 'vue'
const count = ref(0)
const options = { equals: () => true }
watch(count, (n) => void n, options)
</script>

<template>
  <p />
</template>
