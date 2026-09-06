<script setup lang="ts">
import { computed, ref } from 'vue'

const width = ref(10)
const shown = ref(true)
const imageStyle = computed(() => ({ width: `${width.value}px` }))
</script>

<template>
  <div v-if="shown" :style="imageStyle" />
</template>
