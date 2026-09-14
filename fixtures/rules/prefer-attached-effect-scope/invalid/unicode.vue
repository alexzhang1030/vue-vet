<script setup lang="ts">
import { effectScope, onScopeDispose, ref, watch } from 'vue'

const 源 = ref(0)
const 汇 = ref(0)
const 父 = effectScope()
父.run(() => {
  const 子 = effectScope(true)
  子.run(() => {
    watch(源, (value) => { 汇.value = value }, { flush: 'sync' })
  })
  onScopeDispose(() => 子.stop())
})
父.pause()
源.value = 2
源.value = 3
父.resume()
void 汇.value
</script>

<template>
  <p />
</template>
