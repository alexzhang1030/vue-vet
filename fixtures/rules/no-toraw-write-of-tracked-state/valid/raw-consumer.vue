<script setup lang="ts">
import { reactive, toRaw, watchSyncEffect } from 'vue'
const state = reactive({ count: 1 })
watchSyncEffect(() => {
  void toRaw(state).count
})
const raw = toRaw(state)
raw.count = 2
</script>
<template>{{ state }}</template>
