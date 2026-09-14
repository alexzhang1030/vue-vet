<script setup lang="ts">
import { reactive, watchSyncEffect } from 'vue'
const state = reactive({ count: 1 })
watchSyncEffect(() => {
  void state.count
})
state.count = 2
</script>
<template>{{ state }}</template>
