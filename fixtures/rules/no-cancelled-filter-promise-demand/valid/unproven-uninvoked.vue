<script setup lang="ts">
// Unproven quiet control — calls inside an uninvoked function are not yet detected.
import { useDebounceFn } from '@vueuse/core'
const run = useDebounceFn((value: string) => value.toUpperCase(), 50)
async function later() {
  const first = run('aa')
  run('bb')
  ;(await first).slice(0, 1)
}
void later
</script>

<template>
  <p />
</template>
