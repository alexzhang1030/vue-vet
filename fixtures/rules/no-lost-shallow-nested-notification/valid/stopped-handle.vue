<script setup lang="ts">
import { shallowRef, watchSyncEffect } from 'vue'
const stopped = shallowRef({ n: 1 })
const handle = watchSyncEffect(() => { void stopped.value.n })
handle.stop()
stopped.value.n = 2
</script>
<template>{{ stopped }}</template>
