<script setup lang="ts">
import { shallowReactive, watchSyncEffect } from 'vue'
const state = shallowReactive({ details: { count: 1 } })
watchSyncEffect(() => {
  void state.details.count
})
state.details.count = 2
</script>
<template>{{ state }}</template>
