<script setup lang="ts">
import { ref, computed } from 'vue'
const message = ref('hello')
const capitalized = computed(() => message.value.toUpperCase())
</script>
<template>
  <p>{{ capitalized }}</p>
</template>
