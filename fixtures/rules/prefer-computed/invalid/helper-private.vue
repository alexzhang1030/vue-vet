<script setup>
import { ref, watchEffect } from 'vue'
const source = ref(1)
const target = ref(0)
function assign() { target.value = source.value }
watchEffect(() => { assign() })
</script>
<template>{{ target }}</template>
