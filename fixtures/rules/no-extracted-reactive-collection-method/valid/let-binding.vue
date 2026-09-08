<script setup lang="ts">
import { reactive } from 'vue'
let map = reactive(new Map([['a', 1]]))
const { get } = map
get('a')
map = reactive(new Map())
</script>

<template>
  <p />
</template>
