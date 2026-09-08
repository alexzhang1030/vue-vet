<script setup lang="ts">
import { shallowRef as sr, watchSyncEffect as wse } from 'vue'
const state = sr({ count: 1 })
wse(() => {
  void state.value.count
})
state.value.count = 2
</script>
<template>{{ state }}</template>
