<script setup lang="ts">
import { ref } from 'vue'
const createSharedComposable = (factory: (value: string | number) => unknown) => factory
const useValue = createSharedComposable((value) => ref(value))
const first = useValue(1)
const second = useValue('text')
void (second as { value: string }).value.toUpperCase()
</script>

<template>
  <p />
</template>
