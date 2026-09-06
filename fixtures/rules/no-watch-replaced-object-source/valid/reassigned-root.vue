<script setup lang="ts">
import { reactive, watch } from 'vue'
let state = reactive({ p: { x: 1 } })
watch(state.p, () => {})
state = reactive({ p: { x: 2 } })
state.p = { x: 3 }
</script>

<template>
  <p />
</template>
