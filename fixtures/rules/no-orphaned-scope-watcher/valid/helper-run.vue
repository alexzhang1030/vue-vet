<script setup lang="ts">
import { effectScope, ref, watchEffect } from 'vue'
const source = ref(0)
const scope = effectScope()
const helper = {
  run(owner: { run: unknown }) {
    owner.run = () => undefined
  },
}
helper.run(scope)
scope.run(async () => {
  await Promise.resolve()
  watchEffect(() => {
    source.value
  })
})
</script>
<template>{{ source }}</template>
