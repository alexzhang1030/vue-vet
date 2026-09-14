<script setup lang="ts">
import { effectScope, nextTick, ref, watch } from 'vue'

const parent = effectScope()
parent.run(async () => {
  const source = ref(0)
  const sink = ref(0)
  watch(source, (value) => { sink.value = value }, { flush: 'sync' })
  source.value = 1
  source.value = 2
  await nextTick()
  void sink.value
})
parent.stop()
</script>

<template>
  <p />
</template>
