<script setup lang="ts">
import { onMounted } from 'vue'
import { useDebounceFn } from '@vueuse/core'
const run = useDebounceFn((value: string) => value.toUpperCase(), 50)
onMounted(async () => {
  const first = run('aa')
  run('bb')
  ;(await first).slice(0, 1)
})
</script>

<template>
  <p />
</template>
