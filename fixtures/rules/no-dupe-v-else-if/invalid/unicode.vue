<script setup lang="ts">
import { ref } from 'vue'
const type = ref('a')
</script>
<template>
  <p>中文前缀</p>
  <div v-if="type === 'a'">A</div>
  <div v-else-if="type === 'b'">B</div>
  <div v-else-if="type === 'b'">Unreachable</div>
</template>
