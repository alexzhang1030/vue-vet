<script setup lang="ts">
import { ref } from 'vue'
import { useCloned } from '@vueuse/core'
const bag = useCloned(ref({ when: new Date('2020-01-01') }))
bag.cloned.value.when.getTime()
</script>

<template>
  <p />
</template>
