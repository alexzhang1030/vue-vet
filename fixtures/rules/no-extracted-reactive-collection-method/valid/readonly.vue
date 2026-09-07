<script setup lang="ts">
import { readonly } from 'vue'
const map = readonly(new Map([['a', 1]]))
const { get } = map
get('a')
</script>

<template>
  <p />
</template>
