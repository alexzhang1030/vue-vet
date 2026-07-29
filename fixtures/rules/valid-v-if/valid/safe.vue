<script setup lang="ts">
import { ref } from 'vue'
const visible = ref(true)
</script>
<template>
  <div v-if="visible">Shown when visible</div>
</template>
