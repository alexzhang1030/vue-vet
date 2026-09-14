<script setup lang="ts">
import { shallowRef, watchSyncEffect } from 'vue'
const state = shallowRef({ count: 1 })
watchSyncEffect(() => {
  void state.value.count
})
function inner() {
  const state = shallowRef({ count: 9 })
  state.value.count = 2
}
inner()
</script>
<template>{{ state }}</template>
