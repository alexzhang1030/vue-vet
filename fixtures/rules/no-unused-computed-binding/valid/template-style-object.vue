<script setup lang="ts">
import { computed, ref } from 'vue'

const width = ref(10)
const shown = ref(true)
const aspectRatio = computed(() => width.value / 2)
const objectPosition = computed(() => `${width.value}% 50%`)
</script>

<template>
  <div
    v-if="shown"
    :style="{
      aspectRatio,
      objectPosition,
    }"
  />
</template>
