<script setup>
import { ref, watchEffect } from 'vue'
const source = ref(1)
const target = ref(0)
watchEffect(() => { target.value = source.value })
</script>
<template><button @click="'target'">{{ target }}</button></template>
