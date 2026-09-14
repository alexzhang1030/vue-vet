<script setup lang="ts">
import { reactive, watch } from 'vue'
const state = reactive({ p: { x: 1 } })
const flag = true
if (flag) {
  watch(state.p, () => {})
} else {
  state.p = {}
}
</script>

<template>
  <p />
</template>
