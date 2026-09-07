<script setup lang="ts">
import { shallowReactive } from 'vue'
const map = shallowReactive(new Map([['a', 1]]))
const { get } = map
get('a')
</script>

<template>
  <p />
</template>
