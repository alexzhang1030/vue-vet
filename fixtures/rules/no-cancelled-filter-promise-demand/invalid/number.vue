<script setup lang="ts">
import { useDebounceFn } from '@vueuse/core'
const run = useDebounceFn(() => 1.25, 50)
const first = run()
run()
;(await first).toFixed(2)
</script>

<template>
  <p />
</template>
