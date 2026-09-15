<script setup lang="ts">
import { ref } from 'vue'
const items = ref(['a', 'b'])
</script>
<template>
  <p>中文前缀</p>
  <template v-for="item in items" #item="{ item }">{{ item }}</template>
</template>
