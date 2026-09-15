<script setup lang="ts">
import { ref } from 'vue'
import * as VueUse from '@vueuse/core'
const { cloned } = VueUse.useCloned(ref({ when: new Date('2020-01-01') }))
cloned.value.when.getTime()
</script>

<template>
  <p />
</template>
