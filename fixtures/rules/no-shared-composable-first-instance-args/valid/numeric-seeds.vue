<script setup lang="ts">
import { ref } from 'vue'
import { createSharedComposable } from '@vueuse/core'
const useValue = createSharedComposable((value: number) => ref(value))
const first = useValue(1)
const second = useValue(2)
first.value = 3
void second.value.toFixed(0)
</script>

<template>
  <p />
</template>
