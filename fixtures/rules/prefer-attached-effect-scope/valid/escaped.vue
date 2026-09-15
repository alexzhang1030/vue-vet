<script setup lang="ts">
import { effectScope, onScopeDispose, ref, watch } from 'vue'

const source = ref(0)
const sink = ref(0)
const parent = effectScope()
export const child = parent.run(() => {
  const nested = effectScope(true)
  nested.run(() => {
    watch(source, (value) => { sink.value = value }, { flush: 'sync' })
  })
  onScopeDispose(() => nested.stop())
  return nested
})
parent.pause()
source.value = 2
source.value = 3
parent.resume()
void sink.value
</script>

<template>
  <p />
</template>
