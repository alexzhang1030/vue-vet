<script setup lang="ts">
import Child from './Child.vue'
const title = 'hi'
</script>
<template>
  <Child :title="title + '!'" />
</template>
