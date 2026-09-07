<script setup lang="ts">
import { reactive } from 'vue'
const map = reactive(new Map([['a', 1]]))
;(map as { __v_skip?: boolean }).__v_skip = true
const { get } = map
get('a')
</script>

<template>
  <p />
</template>
