<script setup lang="ts">
import { watchEffect } from 'vue'
import { useField } from './composables/useField'

const props = { title: 'Ada' }
const { title } = useField(props)

watchEffect(async () => {
  await Promise.resolve()
  console.log(title.value)
})
</script>

<template>
  <p>{{ title }}</p>
</template>
