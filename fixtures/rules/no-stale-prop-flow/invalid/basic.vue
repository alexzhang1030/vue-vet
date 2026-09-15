<script setup lang="ts">
import Child from './Child.vue'
let title = 'hi'
</script>
<template>
  <Child :title="title" />
</template>
