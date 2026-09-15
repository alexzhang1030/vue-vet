<script setup lang="ts">
import { useDebounceFn } from '@vueuse/core'
const run = useDebounceFn((value: string) => value.toUpperCase(), 50)
const first = await run('aa')
const latest = await run('bb')
first.slice(0, 1)
latest.slice(0, 1)
</script>

<template>
  <p />
</template>
