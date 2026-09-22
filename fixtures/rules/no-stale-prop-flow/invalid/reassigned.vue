<script setup lang="ts">
import Child from './Child.vue'
let title = 'hi'
title = 'bye'
</script>
<template>
  <Child :title="title" />
</template>
