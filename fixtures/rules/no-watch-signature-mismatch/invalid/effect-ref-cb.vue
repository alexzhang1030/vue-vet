<script setup lang="ts">
import { ref, watchEffect } from 'vue'
const count = ref(0)
watchEffect(count, (n) => void n)
</script>

<template>
  <p />
</template>
