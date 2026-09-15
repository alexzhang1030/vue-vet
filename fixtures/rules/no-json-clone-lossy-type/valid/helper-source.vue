<script setup lang="ts">
import { ref } from 'vue'
import { useCloned } from '@vueuse/core'
const make = () => ({ when: new Date('2020-01-01') })
const { cloned } = useCloned(ref(make()))
cloned.value.when.getTime()
</script>

<template>
  <p />
</template>
