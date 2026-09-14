<script setup lang="ts">
import { nextTick, ref, watch } from 'vue'

const source = ref(0)
const sink = ref(0)
watch(source, (value) => { sink.value = value }, { flush: 'sync', once: true })
source.value = 1
source.value = 2
await nextTick()
void sink.value
</script>

<template>
  <p />
</template>
