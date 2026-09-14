<script setup lang="ts">
import { shallowRef, triggerRef, watchSyncEffect } from 'vue'
const state = shallowRef({ count: 1 })
watchSyncEffect(() => {
  void state.value.count
})
state.value.count = 2
triggerRef(state)
</script>
<template>{{ state }}</template>
