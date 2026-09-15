<script setup lang="ts">
import { ref } from 'vue'
import { useMemoize } from '@vueuse/core'
const source = ref(1)
const resolve = useMemoize(() => source.value, { getKey: () => source.value })
resolve()
source.value = 'text'
resolve().toUpperCase()
</script>

<template>
  <p />
</template>
