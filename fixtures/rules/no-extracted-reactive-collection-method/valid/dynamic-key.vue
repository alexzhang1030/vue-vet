<script setup lang="ts">
import { reactive } from 'vue'
const key = 'get'
const map = reactive(new Map([['a', 1]]))
const get = map[key]
get('a')
</script>

<template>
  <p />
</template>
