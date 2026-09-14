<script setup lang="ts">
import { ref, watch, watchEffect } from 'vue'
const count = ref(0)
watch(count, (n) => void n, { flush: 'sync' })
watchEffect(() => void count.value, { flush: 'sync' })
</script>

<template>
  <p />
</template>
