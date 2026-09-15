<script setup lang="ts">
import { onMounted } from 'vue'
const model = defineModel({ default: 1 })
onMounted(() => {
  model.value = 2
})
</script>
<template>
  <div>{{ model }}</div>
</template>
