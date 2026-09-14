<script setup lang="ts">
import { ref, watch } from 'vue'
const count = ref(0)
watch(count, [() => {}, () => {}])
</script>

<template>
  <p />
</template>
