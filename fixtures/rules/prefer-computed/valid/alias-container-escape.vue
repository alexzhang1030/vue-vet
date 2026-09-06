<script setup>
import { ref, watchEffect } from 'vue'
import { mutate } from 'generic-library'
const source = ref(1)
const extra = ref(0)
const target = ref(0)
const alias = target
const state = { alias }
mutate(state)
watchEffect(() => { target.value = source.value + extra.value })
</script>
<template>{{ target }}</template>
