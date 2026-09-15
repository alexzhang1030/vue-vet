<script setup lang="ts">
import { ref } from 'vue'
import { createGlobalState } from '@vueuse/shared'
const useValue = createGlobalState((value: string | number) => ref(value))
const first = useValue(1)
const second = useValue('text')
void second.value.toUpperCase()
</script>

<template>
  <p />
</template>
