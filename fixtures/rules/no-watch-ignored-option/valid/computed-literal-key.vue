<script setup lang="ts">
import { ref, watchEffect } from 'vue'
const count = ref(0)
watchEffect(() => void count.value, { ['once']: true })
</script>

<template>
  <p />
</template>
