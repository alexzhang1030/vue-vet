<script setup lang="ts">
import { reactive } from 'vue'
structuredClone(reactive({ __proto__: { __v_skip: true }, count: 1 }))
</script>

<template>
  <p />
</template>
