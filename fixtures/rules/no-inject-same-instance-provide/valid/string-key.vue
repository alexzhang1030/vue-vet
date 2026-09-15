<script setup lang="ts">
import { inject, provide } from 'vue'
provide('count', 7)
const count = inject('count', 'missing')
count.toFixed(2)
</script>

<template>
  <p />
</template>
