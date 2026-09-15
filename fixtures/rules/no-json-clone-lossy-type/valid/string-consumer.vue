<script setup lang="ts">
import { ref } from 'vue'
import { useCloned } from '@vueuse/core'
const { cloned } = useCloned(ref({ when: new Date('2020-01-01') }))
cloned.value.when.slice(0, 4)
</script>

<template>
  <p />
</template>
