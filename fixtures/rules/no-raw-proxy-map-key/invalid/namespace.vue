<script setup lang="ts">
import * as Vue from 'vue'
const raw = {}
const proxy = Vue.reactive(raw)
const map = new Map([[raw, { count: 1 }]])
void map.get(proxy).count
</script>

<template>
  <p />
</template>
