<script setup lang="ts">
import { reactive, readonly } from 'vue'
const state = reactive({ count: 0 })
const view = readonly(state)
view.count++
</script>
<template>
  <p>中文前缀</p>{{ view.count }}</template>
