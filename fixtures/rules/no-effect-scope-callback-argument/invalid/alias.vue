<script setup lang="ts">
import { effectScope as scope, ref } from 'vue'
const n = ref(0)
scope(() => {
  n.value++
})
</script>

<template>
  <p />
</template>
