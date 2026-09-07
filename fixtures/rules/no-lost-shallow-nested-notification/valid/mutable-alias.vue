<script setup lang="ts">
import { reactive, shallowRef, watchSyncEffect } from 'vue'
const aliased = shallowRef({ n: 0 })
let mutableAlias = aliased
mutableAlias.value = reactive({ n: 0 })
watchSyncEffect(() => { void aliased.value.n })
aliased.value.n = 1
</script>
<template>{{ aliased }}</template>
