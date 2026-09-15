<script setup lang="ts">
import { ref } from 'vue'
import * as VueUse from '@vueuse/core'
const useValue = VueUse.createSharedComposable((value: string | number) => ref(value))
const first = useValue(1)
const second = useValue('text')
void second.value.toUpperCase()
</script>

<template>
  <p />
</template>
