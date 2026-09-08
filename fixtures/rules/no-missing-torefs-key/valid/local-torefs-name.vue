<script setup lang="ts">
import { reactive, toRefs as split } from 'vue'
function toRefs(target: object) {
  Object.assign(target, { added: 7 })
  return {}
}
const state = reactive({ initial: 1 })
toRefs(state)
void split(state).added.value
</script>

<template>
  <p />
</template>
