<script setup lang="ts">
import { ref } from 'vue'
import { useMemoize } from '@vueuse/core'
const String = (value: unknown) => value
const source = ref(1)
const resolve = useMemoize(() => source.value)
resolve()
source.value = 'text'
resolve().toUpperCase()
void String
</script>

<template>
  <p />
</template>
