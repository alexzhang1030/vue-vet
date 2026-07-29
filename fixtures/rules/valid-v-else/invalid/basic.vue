<script setup lang="ts">
import { ref } from 'vue'
const ok = ref(true)
</script>
<template>
  <div v-if="ok">A</div>
  <div v-else="ok">B</div>
</template>
