<script setup lang="ts">
import { ref } from 'vue'
import { createSharedComposable } from '@vueuse/core'
const useValue = createSharedComposable((value: string | number) => ref(value))
const first = useValue(1)
const second = useValue('text')
const third = useValue(2)
third.value = 'fixed'
void second.value.toUpperCase()
</script>

<template>
  <p />
</template>
