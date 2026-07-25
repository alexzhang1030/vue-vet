<script setup lang="ts">
import { ref } from 'vue'

const count = ref(0)
</script>

<template>
  <div v-if="count > 0">{{ count }}</div>
</template>
