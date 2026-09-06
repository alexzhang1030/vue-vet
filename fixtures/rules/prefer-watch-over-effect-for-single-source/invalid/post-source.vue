<script setup>
import { ref, watchPostEffect } from 'vue'
const source = ref(0)
const result = ref(0)
watchPostEffect(() => { result.value = source.value })
</script>
<template>{{ result }}</template>
