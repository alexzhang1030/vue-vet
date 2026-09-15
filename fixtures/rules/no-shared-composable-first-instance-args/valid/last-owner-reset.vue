<script setup lang="ts">
import { effectScope, ref } from 'vue'
import { createSharedComposable } from '@vueuse/core'
const useValue = createSharedComposable((value: string | number) => ref(value))
const firstOwner = effectScope()
firstOwner.run(() => {
  const first = useValue(1)
  void first.value
})
firstOwner.stop()
const secondOwner = effectScope()
secondOwner.run(() => {
  const second = useValue('text')
  void second.value.toUpperCase()
})
</script>

<template>
  <p />
</template>
