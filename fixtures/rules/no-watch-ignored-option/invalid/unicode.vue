<script setup lang="ts">
import { ref, watch } from 'vue'
const 计数 = ref(0)
watch(计数, (n) => void n, { equals: () => true })
</script>

<template>
  <p />
</template>
