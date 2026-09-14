<script setup lang="ts">
import { reactive, toRaw, watchSyncEffect } from 'vue'
const state = reactive({ __v_skip: false, n: 0 })
watchSyncEffect(() => { void state.n })
const raw = toRaw(state)
raw.n = 1
</script>
<template>{{ state }}</template>
