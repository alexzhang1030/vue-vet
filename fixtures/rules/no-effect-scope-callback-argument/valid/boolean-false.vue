<script setup lang="ts">
import { effectScope, ref } from 'vue'
const n = ref(0)
const attached = effectScope(false)
attached.run(() => {
  n.value++
})
</script>

<template>
  <p />
</template>
