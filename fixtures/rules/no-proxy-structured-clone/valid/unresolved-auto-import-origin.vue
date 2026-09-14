<script setup lang="ts">
import { reactive } from '#imports'
structuredClone(reactive({ count: 1 }))
</script>

<template>
  <p />
</template>
