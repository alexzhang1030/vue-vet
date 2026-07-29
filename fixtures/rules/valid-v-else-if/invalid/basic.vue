<script setup lang="ts">
import { ref } from 'vue'
const kind = ref('a')
</script>
<template>
  <div v-if="kind === 'a'">A</div>
  <div v-else-if="">B</div>
</template>
