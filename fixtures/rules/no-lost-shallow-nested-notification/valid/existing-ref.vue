<script setup lang="ts">
import { shallowRef, watchSyncEffect } from 'vue'
const state = shallowRef({ __v_isRef: true, value: { n: 0 } })
watchSyncEffect(() => { void state.value.n })
state.value.n++
</script>
<template>{{ state }}</template>
