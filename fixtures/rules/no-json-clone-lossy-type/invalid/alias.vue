<script setup lang="ts">
import { ref } from 'vue'
import { useCloned as cloneState } from '@vueuse/core'
const { cloned } = cloneState(ref({ when: new Date('2020-01-01') }))
cloned.value.when.getTime()
</script>

<template>
  <p />
</template>
