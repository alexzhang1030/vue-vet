<script setup lang="ts">
import { computed, inject, provide, ref } from 'vue'

// Multiple provides of the same key: inject must stay under-approx quiet.
provide('theme', ref('light'))
provide('theme', ref('dark'))
const theme = inject('theme')
const label = computed(() => theme.value)
void label
</script>

<template>
  <p>ambiguous</p>
</template>
