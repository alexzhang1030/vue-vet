<script setup>
import { ref, watchEffect } from 'vue'
const target = ref(0)
const source = ref(1)
const extra = ref(2)
watchEffect(() => { [target.value] = [source.value + extra.value] })
</script>
<template>{{ target }}</template>
