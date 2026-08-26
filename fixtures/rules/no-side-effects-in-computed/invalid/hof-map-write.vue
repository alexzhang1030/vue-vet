<script setup lang="ts">
import { computed, ref } from 'vue'
const list = ref([0])
const t = ref(0)
const c = computed(() => { list.value.map(() => { t.value = 1 }); return list.value })
</script>
<template>{{ c }}</template>
