<script setup lang="ts">
import { reactive as rx } from 'vue'
const map = rx(new Map([['a', 1]]))
const alias = map
const get = alias.get
get('a')
</script>

<template>
  <p />
</template>
