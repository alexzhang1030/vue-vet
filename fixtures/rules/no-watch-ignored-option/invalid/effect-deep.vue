<script setup lang="ts">
import { ref, watchPostEffect } from 'vue'
const count = ref(0)
watchPostEffect(() => void count.value, { deep: true })
</script>

<template>
  <p />
</template>
