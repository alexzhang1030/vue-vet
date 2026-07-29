<script setup lang="ts">
import { ref } from 'vue'
const items = ref(['a', 'b'])
</script>
<template>
  <template v-for="item in items" #item="{ item }">{{ item }}</template>
</template>
