<script setup lang="ts">
import { ref } from 'vue'
import { useMemoize } from '@vueuse/core'
const source = ref(1)
const resolve = useMemoize(() => source.value)
resolve()
source.value = 'text'
void resolve()
</script>

<template>
  <p />
</template>
