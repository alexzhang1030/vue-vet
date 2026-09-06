<script setup>
import { ref, watchEffect } from 'vue'
const draft = ref({ params: { scheduledAt: '' } })
const time = ref('09:00')
watchEffect(() => {
  draft.value.params.scheduledAt = time.value
})
</script>
<template>{{ draft }}</template>
