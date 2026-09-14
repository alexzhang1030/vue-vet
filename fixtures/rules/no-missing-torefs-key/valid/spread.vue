<script setup lang="ts">
import { reactive, toRefs } from 'vue'
const extra = { extra: 1 }
void toRefs(reactive({ count: 1, ...extra })).missing.value
</script>

<template>
  <p />
</template>
