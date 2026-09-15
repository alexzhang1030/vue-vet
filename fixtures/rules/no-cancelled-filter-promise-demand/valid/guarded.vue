<script setup lang="ts">
import { useDebounceFn } from '@vueuse/core'
const run = useDebounceFn((value: string) => value.toUpperCase(), 50)
const first = run('aa')
run('bb')
const cancelled = await first
if (cancelled) cancelled.slice(0, 1)
</script>

<template>
  <p />
</template>
