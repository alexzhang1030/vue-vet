<script setup lang="ts">
import { shallowReactive } from 'vue'
const state = shallowReactive({ count: 0 })
</script>
<template>{{ state.count }}</template>
