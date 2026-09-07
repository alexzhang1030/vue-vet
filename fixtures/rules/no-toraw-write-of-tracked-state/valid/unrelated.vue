<script setup lang="ts">
import { reactive, toRaw, watchSyncEffect } from 'vue'
const state = reactive({ count: 1, other: 0 })
watchSyncEffect(() => {
  void state.other
})
const raw = toRaw(state)
raw.count = 2
</script>
<template>{{ state }}</template>
