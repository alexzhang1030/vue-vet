<script setup>
import { ref, watchSyncEffect as syncEffect } from 'vue'
const source = ref(0)
const result = ref(0)
syncEffect(() => { result.value = source.value })
</script>
<template>{{ result }}</template>
