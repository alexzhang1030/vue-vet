<script setup lang="ts">
// Unproven quiet control — not a safe pattern.
// Runtime (VueUse 13.9.0): maxWait > 0 fulfills both promises `undefined`.
import { useDebounceFn } from '@vueuse/core'
const run = useDebounceFn((value: string) => value.toUpperCase(), 50, { maxWait: 5 })
const first = run('aa')
run('bb')
;(await first).slice(0, 1)
</script>

<template>
  <p />
</template>
