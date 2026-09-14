<script setup lang="ts">
import * as Vue from 'vue'
void Vue.toRefs(Vue.reactive({ count: 1 })).missing.value
</script>

<template>
  <p />
</template>
