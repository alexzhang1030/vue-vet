<script setup lang="ts">
import { watch } from 'vue'

const file = defineModel<File | null>('file', { default: null })
watch(file, (file) => {
  if (!file)
    return
})
</script>

<template>
  <p />
</template>
