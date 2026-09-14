<script setup lang="ts">
import { ref } from 'vue'
import { useMemoize } from '@vueuse/core'
const source = ref(1)
const resolve = useMemoize(() => source.value)
resolve()
source.value = 'text'
resolve().toString()
</script>

<template>
  <p />
</template>
