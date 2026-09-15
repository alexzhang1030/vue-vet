<script setup lang="ts">
import { shallowReactive } from 'vue'
const { count } = shallowReactive({ count: 0 })
</script>
<template>{{ count }}</template>
