<script setup lang="ts">
import { useDebounceFn } from '@vueuse/core'
const run = useDebounceFn((value: string) => value.toUpperCase(), 50, { rejectOnCancel: true })
const first = run('aa').then(
  (value) => value,
  () => 'cancelled',
)
run('bb')
void (await first)
</script>

<template>
  <p />
</template>
