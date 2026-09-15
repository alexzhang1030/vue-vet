<script setup lang="ts">
import { useDebounceFn } from '@vueuse/core'
function hold(value: Promise<string>) {
  void value
}
const run = useDebounceFn((value: string) => value.toUpperCase(), 50)
const first = run('aa')
run('bb')
hold(first)
;(await first).slice(0, 1)
</script>

<template>
  <p />
</template>
