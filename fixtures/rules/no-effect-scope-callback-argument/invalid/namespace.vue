<script setup lang="ts">
import * as Vue from 'vue'
const n = Vue.ref(0)
Vue.effectScope(() => {
  n.value++
})
</script>

<template>
  <p />
</template>
