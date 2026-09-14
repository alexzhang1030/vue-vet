<script setup lang="ts">
import { ref } from 'vue'
import { syncRef } from '@vueuse/core'

const source = ref(1)
const display = ref(0)
const options = { direction: 'both' as const }
syncRef(source, display, { ...options })
source.value = 5
void display.value
</script>

<template>
  <p />
</template>
