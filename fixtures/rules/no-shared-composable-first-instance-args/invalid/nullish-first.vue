<script setup lang="ts">
import { ref } from 'vue'
import { createSharedComposable } from '@vueuse/core'
const useValue = createSharedComposable((value: string | null) => ref(value))
const first = useValue(null)
const second = useValue('text')
void second.value.toUpperCase()
</script>

<template>
  <p />
</template>
