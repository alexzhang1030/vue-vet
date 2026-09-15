<script setup lang="ts">
import { useDebounceFn } from '@vueuse/core'
const run = useDebounceFn((value: string) => value.toUpperCase(), 20)
const slow = useDebounceFn((value: string) => value, 80)
const first = run('aa')
await slow('x')
run('bb')
;(await first).slice(0, 1)
</script>

<template>
  <p />
</template>
