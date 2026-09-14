<script setup lang="ts">
import { ref } from 'vue'
import { syncRef } from '@vueuse/core'

export const source = ref(1)
export const display = ref(0)
syncRef(source, display)
source.value = 5
void display.value
</script>

<template>
  <p />
</template>
