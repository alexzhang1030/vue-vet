<script setup lang="ts">
import { ref } from 'vue'
import { useCloned } from '@vueuse/core'
const { cloned } = useCloned(ref({ when: new Date('2020-01-01') }) as { value: { when: Date } })
;(cloned.value.when as Date).getTime()
</script>

<template>
  <p />
</template>
