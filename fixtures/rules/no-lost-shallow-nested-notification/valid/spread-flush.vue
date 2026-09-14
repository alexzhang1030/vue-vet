<script setup lang="ts">
import { shallowRef, watchEffect } from 'vue'
const spreadFlush = shallowRef({ n: 1 })
const postOptions = { flush: 'post' }
watchEffect(() => { void spreadFlush.value.n }, { flush: 'pre', ...postOptions })
spreadFlush.value.n = 2
</script>
<template>{{ spreadFlush }}</template>
