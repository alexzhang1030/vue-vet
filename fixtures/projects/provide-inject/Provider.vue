<script setup lang="ts">
import { provide, ref } from 'vue'

const count = ref(1)
provide('count', count)
</script>

<template>
  <p>provider</p>
</template>
