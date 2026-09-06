<script setup lang="ts">
import { reactive, watch } from 'vue'
const published = { n: 0 }
const container = [published]
Object.freeze(container[0])
const publishedState = reactive(published)
function accept(_value: unknown) {}
watch(publishedState, (next, old) => {
  if (next === old) return
  accept(next)
})
</script>

<template>
  <p />
</template>
