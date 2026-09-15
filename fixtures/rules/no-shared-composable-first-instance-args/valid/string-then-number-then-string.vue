<script setup lang="ts">
import { ref } from 'vue'
import { createSharedComposable } from '@vueuse/core'
const useValue = createSharedComposable((value: string | number) => ref(value))
const first = useValue('text')
const second = useValue(1)
const third = useValue('text')
void third.value.toUpperCase()
</script>

<template>
  <p />
</template>
