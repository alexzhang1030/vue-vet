<script setup lang="ts">
import { computed, ref, watch } from 'vue'

const flag = ref(false)
const source = ref(2)
const sink = ref(5)
const heavy = computed(() => source.value)
watch([flag, heavy], ([active, value]) => {
  console.log(value)
  if (active) sink.value = value
})
source.value = 3
</script>

<template>
  <p />
</template>
