<script setup lang="ts">
import { ref } from 'vue'

function useTranslation(status: { language?: string }) {
  if ('language' in status)
    return status.language
}

function other() {
  const status = ref(0)
  return status
}

void useTranslation
void other
</script>

<template>
  <p />
</template>
