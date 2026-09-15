<script setup lang="ts">
import { ref } from 'vue'
import { createGlobalState } from '@vueuse/core'
const useValue = createGlobalState((value: string | number) => ref(value))
const seeded = useValue('seed')
function load() {
  const first = useValue(1)
  const second = useValue('text')
  void second.value.toUpperCase()
}
load()
</script>

<template>
  <p />
</template>
