<script setup lang="ts">
import { reactive } from 'vue-demi'
structuredClone(reactive({ count: 1 }))
</script>

<template>
  <p />
</template>
