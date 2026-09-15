<script setup lang="ts">
import { useDebounceFn } from '@vueuse/core'
String.prototype.slice = function slice() {
  return ''
}
const run = useDebounceFn((value: string) => value.toUpperCase(), 50)
const first = run('aa')
run('bb')
;(await first).slice(0, 1)
</script>

<template>
  <p />
</template>
