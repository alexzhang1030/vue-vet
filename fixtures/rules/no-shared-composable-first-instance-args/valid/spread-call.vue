<script setup lang="ts">
import { ref } from 'vue'
import { createSharedComposable } from '@vueuse/core'
const useValue = createSharedComposable((value: string | number) => ref(value))
const args: [string] = ['seed']
const zero = useValue(...args)
const first = useValue(1)
const second = useValue('text')
void second.value.toUpperCase()
</script>

<template>
  <p />
</template>
