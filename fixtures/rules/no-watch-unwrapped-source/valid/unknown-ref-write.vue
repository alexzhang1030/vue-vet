<script setup lang="ts">
import { ref, watch } from 'vue'
const n = ref(0 as unknown)
n.value = maybeObject()
watch(n.value, () => {})
function maybeObject(): object { return {} }
</script>

<template>
  <p />
</template>
