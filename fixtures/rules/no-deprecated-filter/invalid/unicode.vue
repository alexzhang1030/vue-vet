<script setup lang="ts">
import { ref } from 'vue'
const message = ref('hello')
</script>
<template>
  <p>中文前缀</p>
  <p>{{ message | capitalize }}</p>
</template>
