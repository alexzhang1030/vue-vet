<script setup lang="ts">
import { useDebounceFn } from '@vueuse/core'
const run = useDebounceFn((value: string) => value.toUpperCase(), 20)
const first = run('aa')
await first
run('bb')
;(await first).slice(0, 1)
</script>

<template>
  <p />
</template>
