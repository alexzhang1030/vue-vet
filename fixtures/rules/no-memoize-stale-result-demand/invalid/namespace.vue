<script setup lang="ts">
import { ref } from 'vue'
import * as VueUse from '@vueuse/core'
const source = ref(1)
const resolve = VueUse.useMemoize(() => source.value)
resolve()
source.value = 'text'
resolve().toUpperCase()
</script>

<template>
  <p />
</template>
