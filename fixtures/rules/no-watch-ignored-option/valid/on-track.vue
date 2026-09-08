<script setup lang="ts">
import { ref, watch } from 'vue'
const count = ref(0)
watch(count, (n) => void n, { onTrack() {}, onTrigger() {} })
</script>

<template>
  <p />
</template>
