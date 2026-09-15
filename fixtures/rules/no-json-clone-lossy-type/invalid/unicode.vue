<script setup lang="ts">
import { ref } from 'vue'
import { useCloned } from '@vueuse/core'
const { cloned } = useCloned(ref({ 时刻: new Date('2020-01-01') }))
cloned.value.时刻.getTime()
</script>

<template>
  <p />
</template>
