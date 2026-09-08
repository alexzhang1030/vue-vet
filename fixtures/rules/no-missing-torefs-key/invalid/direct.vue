<script setup lang="ts">
import { reactive, toRefs } from 'vue'
void toRefs(reactive({ count: 1 })).missing.value
</script>

<template>
  <p />
</template>
