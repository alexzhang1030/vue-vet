<script setup lang="ts">
import * as Vue from 'vue'
const count = Vue.customRef(() => ({
  set() {},
}))
void count.value
</script>

<template>
  <p />
</template>
