<script setup lang="ts">
import { ref } from 'vue'
const revision = ref(0)
const label = ref('one')
</script>
<template>
  <div v-memo="[revision]">{{ label }}</div>
</template>
