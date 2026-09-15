<script setup lang="ts">
import { ref } from 'vue'
import { createSharedComposable as shared } from '@vueuse/core'
const useValue = shared((value: string | number) => ref(value))
const first = useValue(1)
const second = useValue('text')
void second.value.toUpperCase()
</script>

<template>
  <p />
</template>
