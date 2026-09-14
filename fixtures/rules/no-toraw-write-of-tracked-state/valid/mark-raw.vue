<script setup lang="ts">
import { markRaw, reactive, toRaw, watchSyncEffect } from 'vue'
const state = reactive(markRaw({ n: 0 }))
watchSyncEffect(() => { void state.n })
toRaw(state).n = 1
</script>
<template>{{ state }}</template>
