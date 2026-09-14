<script setup lang="ts">
import { shallowRef, watchEffect } from 'vue'
const spreadArgs = shallowRef({ n: 1 })
watchEffect(() => { void spreadArgs.value.n }, ...[{ flush: 'post' }])
spreadArgs.value.n = 2
</script>
<template>{{ spreadArgs }}</template>
