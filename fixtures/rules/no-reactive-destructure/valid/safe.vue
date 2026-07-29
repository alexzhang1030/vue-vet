<script setup lang="ts">
import { reactive } from 'vue'
const state = reactive({ count: 0 })
</script>
<template>{{ state.count }}</template>
