<script setup lang="ts">
import { ref } from 'vue'
const message = ref('hello')
</script>
<template>
  <p>{{ message | capitalize }}</p>
</template>
