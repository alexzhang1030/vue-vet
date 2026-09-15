<script setup lang="ts">
import { useDebounceFn } from '@vueuse/core'
const options = { rejectOnCancel: false }
const run = useDebounceFn((value: string) => value.toUpperCase(), 50, options)
const first = run('aa')
run('bb')
;(await first).slice(0, 1)
</script>

<template>
  <p />
</template>
