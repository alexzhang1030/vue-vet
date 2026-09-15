<script setup lang="ts">
// Unproven quiet control — not a safe pattern.
// Runtime (VueUse 13.9.0): trailing/leading-false throttle can cancel a pending promise.
import { useThrottleFn } from '@vueuse/core'
const run = useThrottleFn((value: string) => value.toUpperCase(), 20, true, true)
const first = run('aa')
run('bb')
run('cc')
;(await first).slice(0, 1)
</script>

<template>
  <p />
</template>
