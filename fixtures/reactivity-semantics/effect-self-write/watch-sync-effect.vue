<script setup lang="ts">
import { ref, watchSyncEffect } from 'vue'
const count = ref(0)
watchSyncEffect(() => { count.value = count.value + 1 })
</script>
<template>{{ count }}</template>
