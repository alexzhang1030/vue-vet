<script setup lang="ts">
import * as Vue from 'vue'
const map = Vue.reactive(new Map([['a', 1]]))
const { has } = map
has('a')
</script>

<template>
  <p />
</template>
