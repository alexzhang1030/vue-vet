<script setup lang="ts">
import * as Vue from 'vue'
const count = Vue.ref(0)
Vue.watch(count, (n) => void n, { equals: () => true })
</script>

<template>
  <p />
</template>
