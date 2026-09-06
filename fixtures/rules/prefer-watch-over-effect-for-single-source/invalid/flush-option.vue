<script setup>
import { ref, watchEffect } from 'vue'
const source = ref(0)
const result = ref(0)
watchEffect(() => { result.value = source.value }, { flush: 'post' })
</script>
<template>{{ result }}</template>
