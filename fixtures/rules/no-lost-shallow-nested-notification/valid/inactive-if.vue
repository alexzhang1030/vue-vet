<script setup lang="ts">
import { shallowRef, watchSyncEffect } from 'vue'
const inactive = shallowRef({ n: 1 })
if (false) watchSyncEffect(() => { void inactive.value.n })
inactive.value.n = 2
</script>
<template>{{ inactive }}</template>
