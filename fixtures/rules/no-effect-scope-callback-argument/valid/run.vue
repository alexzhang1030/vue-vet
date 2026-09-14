<script setup lang="ts">
import { effectScope, ref } from 'vue'
const n = ref(0)
const scope = effectScope()
scope.run(() => {
  n.value++
})
</script>

<template>
  <p />
</template>
