<script setup lang="ts">
import { shallowRef, watchSyncEffect } from 'vue'
const deferred = shallowRef({ n: 1 })
watchSyncEffect(async () => {
  await Promise.resolve()
  void deferred.value.n
})
deferred.value.n = 2
</script>
<template>{{ deferred }}</template>
