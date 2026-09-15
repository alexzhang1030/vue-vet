<script setup lang="ts">
import { customRef, watch } from 'vue'
const count = customRef(() => ({ get: 1, set: 2 }))
watch(count, () => {})
count.value = 1
</script>

<template>
  <p />
</template>
