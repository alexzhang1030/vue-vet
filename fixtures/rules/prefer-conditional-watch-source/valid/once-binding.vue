<script setup lang="ts">
import { computed, ref, watch } from 'vue'

const flag = ref(false)
const source = ref(2)
const sink = ref(5)
const oneRun = true
const heavy = computed(() => source.value)
watch(
  [flag, heavy],
  ([active, value]) => {
    if (active) sink.value = value
  },
  { flush: 'sync', once: oneRun },
)
source.value = 3
function enable(target: { value: boolean }) {
  target.value = true
}
enable(flag)
void sink.value
</script>

<template>
  <p />
</template>
