<script setup lang="ts">
import { ref } from 'vue'
import { useCloned } from '@vueuse/core'
class Date {
  getTime() {
    return 1
  }
}
const { cloned } = useCloned(ref({ when: new Date() }))
cloned.value.when.getTime()
</script>

<template>
  <p />
</template>
