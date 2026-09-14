<script setup lang="ts">
import { reactive } from 'vue'
declare const extra: { count: number }
structuredClone(reactive({ ...extra }))
</script>

<template>
  <p />
</template>
