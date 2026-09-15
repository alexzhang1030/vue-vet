<script setup lang="ts">
import { useDebounceFn } from '@vueuse/core'
const extra = { maxWait: 0 }
const run = useDebounceFn((value: string) => value.toUpperCase(), 50, { ...extra })
const first = run('aa')
run('bb')
;(await first).slice(0, 1)
</script>

<template>
  <p />
</template>
