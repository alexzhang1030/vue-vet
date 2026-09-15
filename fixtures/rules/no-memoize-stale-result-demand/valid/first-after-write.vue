<script setup lang="ts">
import { ref } from 'vue'
import { useMemoize } from '@vueuse/core'
const source = ref(1)
const resolve = useMemoize(() => source.value)
source.value = 'text'
resolve().toUpperCase()
</script>

<template>
  <p />
</template>
