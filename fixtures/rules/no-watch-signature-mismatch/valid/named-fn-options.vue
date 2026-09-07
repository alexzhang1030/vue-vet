<script setup lang="ts">
import { ref, watchEffect } from 'vue'
const count = ref(0)
function options() {}
;(options as { flush?: string }).flush = 'sync'
watchEffect(() => void count.value, options)
</script>

<template>
  <p />
</template>
