<script setup lang="ts">
import { useDebounceFn } from '@vueuse/core'
function hold(fn: (value: string) => Promise<string>) {
  void fn
}
const run = useDebounceFn((value: string) => value.toUpperCase(), 50)
hold(run)
const first = run('aa')
run('bb')
;(await first).slice(0, 1)
</script>

<template>
  <p />
</template>
