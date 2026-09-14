<script setup lang="ts">
import { toRef } from 'vue'
const getter = toRef(() => 1, 'n')
void getter
</script>

<template>
  <p />
</template>
