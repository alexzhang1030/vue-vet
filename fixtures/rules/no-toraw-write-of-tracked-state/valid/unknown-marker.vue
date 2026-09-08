<script setup lang="ts">
import { reactive, toRaw, watchSyncEffect } from 'vue'
const flag = 1
const state = reactive({ __v_skip: flag, n: 0 })
watchSyncEffect(() => { void state.n })
toRaw(state).n++
</script>
<template>{{ state }}</template>
