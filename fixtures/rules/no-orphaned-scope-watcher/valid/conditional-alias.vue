<script setup lang="ts">
import { effectScope, ref, watchEffect } from 'vue'
const source = ref(0)
const scope = effectScope()
const alias = true ? scope : effectScope()
alias.run = () => undefined
scope.run(async () => {
  await Promise.resolve()
  watchEffect(() => {
    source.value
  })
})
</script>
<template>{{ source }}</template>
