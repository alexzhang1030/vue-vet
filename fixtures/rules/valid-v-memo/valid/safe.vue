<script setup lang="ts">
import { ref } from 'vue'
const value = ref(true)
</script>
<template>
  <div v-memo="[value]">content</div>
</template>
