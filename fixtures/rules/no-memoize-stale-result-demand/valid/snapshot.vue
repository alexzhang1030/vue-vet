<script setup lang="ts">
import { ref } from 'vue'
import { useMemoize } from '@vueuse/core'
const source = ref(1)
const resolve = useMemoize(() => source.value)
const snapshot = resolve()
source.value = 'text'
snapshot.toString()
</script>

<template>
  <p />
</template>
