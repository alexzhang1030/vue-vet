<script setup lang="ts">
// Unproven quiet control — only a call-site literal delay is proven.
import { ref } from 'vue'
import { useDebounceFn } from '@vueuse/core'
const delay = ref(50)
const run = useDebounceFn((value: string) => value.toUpperCase(), delay)
const first = run('aa')
run('bb')
;(await first).slice(0, 1)
</script>

<template>
  <p />
</template>
