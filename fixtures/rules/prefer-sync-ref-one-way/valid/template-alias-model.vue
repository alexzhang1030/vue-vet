<script setup lang="ts">
import { ref } from 'vue'
import { syncRef } from '@vueuse/core'

const source = ref(1)
const display = ref(0)
const alias = display
syncRef(source, display)
source.value = 5
void display.value
</script>

<template>
  <input v-model="alias" />
</template>
