<script setup lang="ts">
import { ref, watch, watchEffect } from 'vue'
const count = ref(0)
watch(count, (n) => void n)
watchEffect(() => void count.value)
</script>

<template>
  <p />
</template>
