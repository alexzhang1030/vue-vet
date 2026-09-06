<script setup>
import { ref, watchEffect } from 'vue'
const source = ref(0)
const result = ref(0)
const rest = {}
const options = { flush: 'sync' }
const key = 'other'
watchEffect(() => { result.value = source.value }, options)
watchEffect(() => { result.value = source.value }, { flush: 'pre', ...rest })
watchEffect(() => { result.value = source.value }, { ...rest, flush: 'sync' })
watchEffect(() => { result.value = source.value }, { flush: 'sync', [key]: true })
</script>
<template>{{ result }}</template>
