<script setup lang="ts">
import { shallowReactive } from 'vue'
const { count } = shallowReactive({ count: 0 })
</script>
<template>
  <p>中文前缀</p>{{ count }}</template>
