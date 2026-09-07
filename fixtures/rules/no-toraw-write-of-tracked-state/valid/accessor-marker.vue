<script setup lang="ts">
import { reactive, toRaw, watchSyncEffect } from 'vue'
const state = reactive({ get __v_isReadonly() { return 'yes' }, n: 0 })
watchSyncEffect(() => { void state.n })
toRaw(state).n++
</script>
<template>{{ state }}</template>
