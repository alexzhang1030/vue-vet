<script setup lang="ts">
import { reactive } from 'vue'
structuredClone(reactive({ count: 1 }) as { count: number })
</script>

<template>
  <p />
</template>
