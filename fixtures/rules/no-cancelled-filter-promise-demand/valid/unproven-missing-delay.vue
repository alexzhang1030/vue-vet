<script setup lang="ts">
// Unproven quiet control — missing delay (VueUse default 200) is not a call-site literal.
import { useDebounceFn } from '@vueuse/core'
const run = useDebounceFn((value: string) => value.toUpperCase())
const first = run('aa')
run('bb')
;(await first).slice(0, 1)
</script>

<template>
  <p />
</template>
