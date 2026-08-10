<script setup lang="ts">
const text = defineModel<string>()
</script>

<template>
  <input v-model="text" />
</template>
