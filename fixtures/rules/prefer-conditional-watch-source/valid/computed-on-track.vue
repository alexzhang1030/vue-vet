<script setup lang="ts">
import { computed, ref, watch } from 'vue'

const flag = ref(false)
const source = ref(2)
const sink = ref(5)
const events: string[] = []
const heavy = computed(() => source.value, {
  onTrack: (event) => events.push(event.type),
})
watch(
  [flag, heavy],
  ([active, value]) => {
    if (active) sink.value = value
  },
  { flush: 'sync' },
)
source.value = 3
</script>

<template>
  <p />
</template>
