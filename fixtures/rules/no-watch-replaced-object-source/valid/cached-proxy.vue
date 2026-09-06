<script setup lang="ts">
import { reactive, watch } from 'vue'
const obj = {}
const state = reactive({ p: reactive(obj) })
watch(state.p, () => {})
state.p = reactive(obj)
</script>

<template>
  <p />
</template>
