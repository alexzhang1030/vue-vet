<script setup lang="ts">
import { ref } from 'vue'
import { useMemoize } from '@vueuse/core'
Number.prototype.toUpperCase = function () {
  return 'supported'
}
const source = ref(1)
const resolve = useMemoize(() => source.value)
resolve()
source.value = 'latest'
resolve().toUpperCase()
</script>

<template>
  <p />
</template>
