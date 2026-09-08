<script setup lang="ts">
import { reactive, toRaw, watchSyncEffect } from 'vue'
const state = reactive({ count: 1 })
watchSyncEffect(() => {
  void state.count
})
const raw = toRaw(state)
const copy = raw
copy.count = 2
</script>
<template>{{ state }}</template>
