<script setup lang="ts">
import { effectScope, ref } from 'vue'
const n = ref(0)
const run = () => {
  n.value++
}
effectScope(run)
</script>

<template>
  <p />
</template>
