<script setup lang="ts">
import { ref } from 'vue'
import { useMemoize } from '@vueuse/core'
const source = ref('initial')
const resolve = useMemoize(() => source.value)
resolve()
source.value = 1
resolve()
source.value = 'latest'
resolve().toUpperCase()
</script>

<template>
  <p />
</template>
