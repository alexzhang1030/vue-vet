<script setup lang="ts">
import { reactive, toRaw, watchSyncEffect } from 'vue'
const state = reactive({ __v_skip: 1, n: 0 })
watchSyncEffect(() => { void state.n })
const raw = toRaw(state)
raw.n++
</script>
<template>{{ state }}</template>
