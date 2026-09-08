<script setup lang="ts">
import { reactive, toRefs } from 'vue'
void toRefs(reactive({ 计数: 1 })).missing.value
</script>

<template>
  <p />
</template>
