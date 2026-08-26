<script setup lang="ts">
import { ref, watch } from 'vue'

const count = ref(0)
watch((count), () => {})
watch(count as any, () => {})
</script>

<template>
  <p>{{ count }}</p>
</template>
