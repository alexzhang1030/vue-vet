<script setup lang="ts">
import { reactive, shallowReactive, watchSyncEffect } from 'vue'
const state = shallowReactive({ details: reactive({ count: 1 }) })
watchSyncEffect(() => {
  void state.details.count
})
state.details.count = 2
</script>
<template>{{ state }}</template>
