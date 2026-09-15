<script setup lang="ts">
import { ref } from 'vue'
import { useCloned } from '@vueuse/core'
const { cloned } = useCloned(ref({
  get when() {
    return new Date('2020-01-01')
  },
}))
cloned.value.when.getTime()
</script>

<template>
  <p />
</template>
