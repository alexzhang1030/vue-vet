<script setup lang="ts">
import { ref } from 'vue'
import { syncRef } from '@vueuse/core'

const source = ref(1)
const display = ref(0)
const seen = display
syncRef(source, display)
source.value = 5
void seen.value
</script>

<template>
  <p />
</template>
