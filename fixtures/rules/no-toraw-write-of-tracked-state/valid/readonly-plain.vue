<script setup lang="ts">
import { readonly, toRaw, watchSyncEffect } from 'vue'
const state = readonly({ count: 1 })
watchSyncEffect(() => {
  void state.count
})
const raw = toRaw(state)
raw.count = 2
</script>
<template>{{ state }}</template>
