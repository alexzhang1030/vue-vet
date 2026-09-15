<script setup lang="ts">
import { useDebounceFn } from '@vueuse/core'
const run = useDebounceFn((value: string) => value.toUpperCase(), 50)
const a = run('aa')
const b = run('bb')
run('cc')
;(await a).slice(0, 1)
;(await b).slice(0, 1)
</script>

<template>
  <p />
</template>
