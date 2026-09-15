<script setup lang="ts">
import { reactive, readonly } from 'vue'
const state = reactive({ count: 0 })
const view = readonly(state)
view.count++
</script>
<template>{{ view.count }}</template>
