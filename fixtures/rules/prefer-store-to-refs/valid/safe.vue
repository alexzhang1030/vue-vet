<script setup lang="ts">
import { storeToRefs } from 'pinia'
const store = useCounterStore()
const { count } = storeToRefs(store)
</script>
<template>{{ count }}</template>
