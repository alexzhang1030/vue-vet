<script setup lang="ts">
import { useRoute } from 'vue-router'
const { params } = useRoute()
</script>
<template>
  <p>中文前缀</p>{{ params }}</template>
