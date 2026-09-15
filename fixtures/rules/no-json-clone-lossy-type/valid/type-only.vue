<script setup lang="ts">
import { ref } from 'vue'
import type { useCloned } from '@vueuse/core'
const useCloned = (value: unknown) => ({ cloned: value as { value: { when: Date } } })
const { cloned } = useCloned(ref({ when: new Date('2020-01-01') }))
cloned.value.when.getTime()
</script>

<template>
  <p />
</template>
