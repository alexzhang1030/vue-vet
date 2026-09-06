<script setup lang="ts">
import { shallowRef, watchSyncEffect } from 'vue'
const state = shallowRef({ count: 1 })
watchSyncEffect(() => {
  void state.value.count
})
state.value.count = 2
</script>
<template>{{ state }}</template>
