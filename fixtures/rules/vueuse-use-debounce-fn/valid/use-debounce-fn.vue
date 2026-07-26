<script setup lang="ts">
import { useDebounceFn } from '@vueuse/core'

const search = useDebounceFn((query: string) => {
  console.log(query)
}, 200)
void search
</script>

<template>
  <input>
</template>
