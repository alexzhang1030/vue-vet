<script setup lang="ts">
import { reactive, toRefs } from 'vue'
const { count } = toRefs(reactive({ count: 1 }))
void count.value
</script>

<template>
  <p />
</template>
