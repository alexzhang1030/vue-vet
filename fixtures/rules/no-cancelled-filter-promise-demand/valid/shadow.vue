<script setup lang="ts">
function useDebounceFn(fn: (value: string) => string, _ms: number) {
  return fn
}
const run = useDebounceFn((value: string) => value.toUpperCase(), 50)
const first = run('aa')
run('bb')
void first.slice(0, 1)
</script>

<template>
  <p />
</template>
