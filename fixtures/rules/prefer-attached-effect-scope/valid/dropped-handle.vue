<script setup lang="ts">
import { effectScope, ref, watch } from 'vue'

const source = ref(0)
const sink = ref(0)
const parent = effectScope()
parent.run(() => {
  const child = effectScope(true)
  child.run(() => {
    watch(source, (value) => { sink.value = value }, { flush: 'sync' })
  })
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
