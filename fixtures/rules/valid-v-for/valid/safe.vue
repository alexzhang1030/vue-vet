<script setup lang="ts">
import { ref } from 'vue'
const items = ref(['a', 'b', 'c'])
</script>
<template>
  <li v-for="item in items" :key="item">{{ item }}</li>
</template>
