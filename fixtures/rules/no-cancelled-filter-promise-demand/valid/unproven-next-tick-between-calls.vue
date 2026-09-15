<script setup lang="ts">
// Unproven quiet control — an await (including `nextTick`) is a region boundary.
// Runtime (VueUse 13.9.0): a microtask between calls can still let the second
// call supersede the first before the debounce timer fires.
import { nextTick } from 'vue'
import { useDebounceFn } from '@vueuse/core'
const run = useDebounceFn((value: string) => value.toUpperCase(), 50)
const first = run('aa')
await nextTick()
run('bb')
;(await first).slice(0, 1)
</script>

<template>
  <p />
</template>
