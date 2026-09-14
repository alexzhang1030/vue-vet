<script setup lang="ts">
import { shallowRef, watchSyncEffect } from 'vue'
const state = shallowRef({ __v_isRef: 1, value: { n: 0 } })
watchSyncEffect(() => { void state.value.value.n })
state.value.value.n = 1
</script>
<template>{{ state }}</template>
