<script setup lang="ts">
import { reactive, toRef } from 'vue'
const future = toRef(reactive({ count: 1 }), 'future')
void future.value
</script>

<template>
  <p />
</template>
