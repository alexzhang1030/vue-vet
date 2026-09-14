<script setup lang="ts">
import { reactive } from 'vue'
function structuredClone(_value: unknown) {}
structuredClone(reactive({ count: 1 }))
</script>

<template>
  <p />
</template>
