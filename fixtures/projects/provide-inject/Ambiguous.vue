<script setup lang="ts">
import { inject, provide, ref } from 'vue'

// Multiple provides of the same key: inject must stay under-approx quiet.
provide('theme', ref('light'))
provide('theme', ref('dark'))
const theme = inject('theme')
</script>

<template>
  <p>{{ theme }}</p>
</template>
