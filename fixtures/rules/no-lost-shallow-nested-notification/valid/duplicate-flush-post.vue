<script setup lang="ts">
import { shallowRef, watchEffect } from 'vue'
const duplicateFlush = shallowRef({ n: 1 })
watchEffect(() => { void duplicateFlush.value.n }, { flush: 'pre', flush: 'post' })
duplicateFlush.value.n = 2
</script>
<template>{{ duplicateFlush }}</template>
