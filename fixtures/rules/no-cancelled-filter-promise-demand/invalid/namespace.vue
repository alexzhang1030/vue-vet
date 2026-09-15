<script setup lang="ts">
import * as VueUse from '@vueuse/core'
const run = VueUse.useDebounceFn((value: string) => value.toUpperCase(), 50)
const first = run('aa')
run('bb')
;(await first).slice(0, 1)
</script>

<template>
  <p />
</template>
