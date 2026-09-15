<script setup lang="ts">
import { ref, shallowRef } from 'vue'
import { syncRef } from '@vueuse/core'

const source = shallowRef(1)
const display = ref(0)
syncRef(source, display)
source.value = 5
void display.value
</script>

<template>
  <p />
</template>
