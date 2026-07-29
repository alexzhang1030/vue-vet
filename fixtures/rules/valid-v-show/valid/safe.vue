<script setup lang="ts">
import { ref } from 'vue'
const visible = ref(true)
</script>
<template>
  <div v-show="visible">Toggled by display</div>
</template>
