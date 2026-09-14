<script setup lang="ts">
import { reactive, watch } from 'vue'
const state = reactive({ p: { x: 1 } })
const stop = watch(state.p, () => {})
stop()
state.p = {}
</script>

<template>
  <p />
</template>
