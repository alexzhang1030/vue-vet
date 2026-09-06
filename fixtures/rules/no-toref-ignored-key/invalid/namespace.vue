<script setup lang="ts">
import * as Vue from 'vue'
const count = Vue.ref(0)
const aliased = Vue.toRef(count, 'n')
void aliased
</script>

<template>
  <p />
</template>
