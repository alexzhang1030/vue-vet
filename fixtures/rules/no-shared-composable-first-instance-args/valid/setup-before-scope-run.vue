<script setup lang="ts">
import { effectScope, ref } from 'vue'
import { createSharedComposable } from '@vueuse/core'
const useValue = createSharedComposable((value: string | number) => ref(value))
const seeded = useValue('seed')
const owner = effectScope()
owner.run(() => {
  const first = useValue(1)
  const second = useValue('text')
  void second.value.toUpperCase()
})
</script>

<template>
  <p />
</template>
