<script setup lang="ts">
import { ref, watch } from 'vue'
const count = ref(0)
watch(count, 'onChange')
</script>

<template>
  <p />
</template>
