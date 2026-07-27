<script setup lang="ts">
import { ref } from 'vue'
import Child from './Child.vue'

const label = ref('hello')
</script>

<template>
  <Child :title="label" />
</template>
