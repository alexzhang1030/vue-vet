<script setup lang="ts">
import { ref } from 'vue'
import { useCloned } from '@vueuse/core'
const extra = { deep: true }
const { cloned } = useCloned(ref({ when: new Date('2020-01-01') }), { ...extra })
cloned.value.when.getTime()
</script>

<template>
  <p />
</template>
