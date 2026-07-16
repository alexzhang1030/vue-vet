<script setup lang="ts">
import { ref } from 'vue'

const { title } = defineProps<{ title: string }>()
const input = ref(null)
console.log(title)
</script>

<template>
  <label>
    {{ title }}
    <input ref="input">
  </label>
</template>
