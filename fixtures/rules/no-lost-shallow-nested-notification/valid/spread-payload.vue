<script setup lang="ts">
import { reactive, shallowRef, watchSyncEffect } from 'vue'
const spread = shallowRef({ item: { n: 0 }, ...{ item: reactive({ n: 0 }) } })
watchSyncEffect(() => { void spread.value.item.n })
spread.value.item.n = 1
</script>
<template>{{ spread }}</template>
