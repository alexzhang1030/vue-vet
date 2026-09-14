<script setup lang="ts">
import { reactive, readonly } from 'vue'
const state = reactive({ count: 1 })
function deep() {
  return structuredClone(readonly(readonly(readonly(readonly(readonly(readonly(readonly(state))))))))
}
function direct() {
  return structuredClone(state)
}
</script>

<template>
  <p />
</template>
