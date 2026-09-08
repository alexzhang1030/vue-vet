<script setup lang="ts">
import { reactive } from 'vue'
const map = reactive(new Map([['a', 1]]))
const { get } = map
Reflect.apply(get, map, ['a'])
</script>

<template>
  <p />
</template>
