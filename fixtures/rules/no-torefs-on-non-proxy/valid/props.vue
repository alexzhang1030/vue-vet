<script setup lang="ts">
import { toRefs } from 'vue'
const props = defineProps<{ a: number }>()
const { a } = toRefs(props)
void a
</script>

<template>
  <p />
</template>
