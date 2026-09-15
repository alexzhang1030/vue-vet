<script setup lang="ts">
import { useDebounceFn as debounce } from '@vueuse/core'
const run = debounce((value: string) => value.toUpperCase(), 50)
const alias = run
const first = alias('aa')
alias('bb')
;(await first).slice(0, 1)
</script>

<template>
  <p />
</template>
