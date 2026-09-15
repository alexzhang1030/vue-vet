<script setup lang="ts">
import { ref } from 'vue'
import * as VueUse from '@vueuse/core'
const source = ref(0)
const value = await VueUse.until(source).toBe('ready', { timeout: 5 })
value.toUpperCase()
</script>

<template>
  <p />
</template>
