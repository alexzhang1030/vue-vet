<script setup lang="ts">
import { shallowRef, watchSyncEffect } from 'vue'
const café = shallowRef({ count: 1 })
watchSyncEffect(() => {
  void café.value.count
})
café.value.count = 2
</script>
<template>{{ café }}</template>
