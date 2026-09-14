<script setup lang="ts">
import { computed, ref, watch } from 'vue'

const flag = ref(false)
const source = ref(2)
const sink = ref(5)
const heavy = computed(() => source.value)
function enable(target: { value: boolean }) {
  target.value = true
}
enable(flag)
watch([flag, heavy], ([active, value]) => {
  if (active) sink.value = value
}, { flush: 'sync' })
source.value = 3
</script>

<template>
  <p />
</template>
