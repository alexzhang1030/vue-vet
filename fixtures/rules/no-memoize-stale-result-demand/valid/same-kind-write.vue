<script setup lang="ts">
import { ref } from 'vue'
import { useMemoize } from '@vueuse/core'
const source = ref(1)
const resolve = useMemoize(() => source.value)
resolve()
source.value = 2
resolve().toUpperCase()
</script>

<template>
  <p />
</template>
