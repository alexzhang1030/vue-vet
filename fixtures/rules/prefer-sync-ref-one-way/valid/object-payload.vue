<script setup lang="ts">
import { ref } from 'vue'
import { syncRef } from '@vueuse/core'

const source = ref({ n: 1 })
const display = ref({ n: 0 })
syncRef(source, display)
source.value = { n: 5 }
void display.value
</script>

<template>
  <p />
</template>
