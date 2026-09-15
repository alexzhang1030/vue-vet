<script setup lang="ts">
// Unproven quiet control — not a safe pattern.
// Runtime (VueUse 13.9.0): `.then` on the cancelled promise is an unhandled rejection.
import { useDebounceFn } from '@vueuse/core'
const run = useDebounceFn((value: string) => value.toUpperCase(), 50)
const first = run('aa')
run('bb')
first.then((value) => value.slice(0, 1))
</script>

<template>
  <p />
</template>
