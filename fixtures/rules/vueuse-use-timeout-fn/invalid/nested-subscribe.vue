<script setup lang="ts">
import { onMounted } from 'vue'

const bus = {
  subscribe(handler: () => void) {
    handler()
  },
}

onMounted(() => {
  bus.subscribe(() => {
    setTimeout(() => {
      console.log('tick')
    }, 1000)
  })
})
</script>

<template>
  <div />
</template>
