<script setup lang="ts">
import * as Vue from 'vue'
const key = Symbol('count')
Vue.provide(key, 7)
const count = Vue.inject(key, 'missing')
count.toFixed(2)
</script>

<template>
  <p />
</template>
