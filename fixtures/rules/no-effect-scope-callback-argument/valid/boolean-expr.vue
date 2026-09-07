<script setup lang="ts">
import { effectScope, ref } from 'vue'
const n = ref(0)
const detached = true
const scope = effectScope(detached)
scope.run(() => {
  n.value++
})
</script>

<template>
  <p />
</template>
