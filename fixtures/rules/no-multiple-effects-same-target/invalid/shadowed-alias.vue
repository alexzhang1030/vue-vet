<script setup>
import { ref, watchEffect } from 'vue'
const source = ref(1)
const state = ref(0)
function createIndependent() { const state = ref(0); return state }
const alias = state
watchEffect(() => { state.value = source.value })
watchEffect(() => { alias.value = source.value + 1 })
</script>
<template>{{ state }}{{ createIndependent() }}</template>
