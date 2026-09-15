<script setup lang="ts">
import { useDebounceFn } from '@vueuse/core'
const run = useDebounceFn((value: string) => value.toUpperCase(), 50)
const first = run('aa')
const latest = run('bb')
void latest
;(await first).slice(0, 1)
</script>

<template>
  <p />
</template>
