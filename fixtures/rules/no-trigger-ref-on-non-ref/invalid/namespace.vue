<script setup lang="ts">
import * as Vue from 'vue'
const obj = Vue.reactive({ n: 1 })
Vue.triggerRef(obj)
</script>

<template>
  <p />
</template>
