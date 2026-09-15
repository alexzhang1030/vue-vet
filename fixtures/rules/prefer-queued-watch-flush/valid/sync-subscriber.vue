<script setup lang="ts">
import { nextTick, ref, watch, watchSyncEffect } from 'vue'

const source = ref(0)
const sink = ref(0)
watch(source, (value) => { sink.value = value }, { flush: 'sync' })
watchSyncEffect(() => { void sink.value })
source.value = 1
source.value = 2
await nextTick()
void sink.value
</script>

<template>
  <p />
</template>
