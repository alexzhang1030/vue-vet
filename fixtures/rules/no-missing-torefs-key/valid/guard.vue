<script setup lang="ts">
import { reactive, toRefs } from 'vue'
const refs = toRefs(reactive({ count: 1 }))
if (refs.missing) void refs.missing.value
</script>

<template>
  <p />
</template>
