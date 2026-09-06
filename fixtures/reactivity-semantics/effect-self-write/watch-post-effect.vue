<script setup lang="ts">
import { ref, watchPostEffect } from 'vue'
const count = ref(0)
watchPostEffect(() => { count.value = count.value + 1 })
</script>
<template>{{ count }}</template>
