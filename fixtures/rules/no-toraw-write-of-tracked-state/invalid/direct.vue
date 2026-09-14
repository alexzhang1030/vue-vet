<script setup lang="ts">
import { reactive, toRaw, watchSyncEffect } from 'vue'
const state = reactive({ count: 1 })
watchSyncEffect(() => {
  void state.count
})
toRaw(state).count = 2
</script>
<template>{{ state }}</template>
