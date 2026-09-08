<script setup lang="ts">
import { reactive, shallowRef, watchSyncEffect } from 'vue'
const duplicate = shallowRef({ item: { deep: { n: 0 } }, item: reactive({ deep: { n: 0 } }) })
watchSyncEffect(() => { void duplicate.value.item.deep.n })
duplicate.value.item.deep.n = 1
</script>
<template>{{ duplicate }}</template>
