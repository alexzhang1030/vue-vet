<script setup lang="ts">
import { reactive, toRefs as split } from 'vue'
void split(reactive({ count: 1 })).missing.value
</script>

<template>
  <p />
</template>
