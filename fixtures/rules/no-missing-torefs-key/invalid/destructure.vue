<script setup lang="ts">
import { reactive, toRefs } from 'vue'
const { missing } = toRefs(reactive({ count: 1 }))
void missing.value
</script>

<template>
  <p />
</template>
