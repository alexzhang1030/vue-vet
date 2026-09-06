<script setup lang="ts">
import { ref, watch } from 'vue'

const enabled = ref(false)
watch(enabled, (enabled) => {
  if (!enabled)
    return
})
</script>

<template>
  <p>{{ enabled }}</p>
</template>
