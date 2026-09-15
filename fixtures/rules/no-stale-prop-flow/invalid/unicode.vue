<script setup lang="ts">
import Child from './Child.vue'
let title = 'hi'
</script>
<template>
  <p>中文前缀</p>
  <Child :title="title" />
</template>
