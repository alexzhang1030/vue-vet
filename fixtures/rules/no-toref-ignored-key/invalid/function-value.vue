<script setup lang="ts">
import { toRef } from 'vue'
const getter = toRef(() => 1, 'value')
void getter
</script>

<template>
  <p />
</template>
