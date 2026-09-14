<script setup lang="ts">
import { ref, watch } from 'vue'
const count = ref(0)
watch(count, (n) => void n, { __proto__: null, equals: () => true })
</script>

<template>
  <p />
</template>
