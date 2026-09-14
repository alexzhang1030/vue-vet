<script setup lang="ts">
import { reactive, shallowRef, watchSyncEffect } from 'vue'
const replaced = shallowRef({ n: 0 })
;({ x: replaced.value } = { x: reactive({ n: 0 }) })
watchSyncEffect(() => { void replaced.value.n })
replaced.value.n = 1
</script>
<template>{{ replaced }}</template>
