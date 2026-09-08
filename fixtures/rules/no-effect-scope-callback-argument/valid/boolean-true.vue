<script setup lang="ts">
import { effectScope, ref } from 'vue'
const n = ref(0)
const detached = effectScope(true)
detached.run(() => {
  n.value++
})
</script>

<template>
  <p />
</template>
