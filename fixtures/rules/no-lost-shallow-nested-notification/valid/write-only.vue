<script setup lang="ts">
import { shallowRef, watchSyncEffect } from 'vue'
const writeOnly = shallowRef({ n: 1 })
watchSyncEffect(() => { writeOnly.value.n = 2 })
writeOnly.value.n = 3
</script>
<template>{{ writeOnly }}</template>
