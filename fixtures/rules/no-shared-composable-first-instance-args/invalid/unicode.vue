<script setup lang="ts">
import { ref } from 'vue'
import { createSharedComposable } from '@vueuse/core'
const 共享 = createSharedComposable((value: string | number) => ref(value))
const 首次 = 共享(1)
const 第二次 = 共享('text')
void 第二次.value.toUpperCase()
</script>

<template>
  <p />
</template>
