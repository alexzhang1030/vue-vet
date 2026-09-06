<script setup lang="ts">
const open = defineModel<boolean>({ default: false })
const shown = open && true
</script>

<template>
  <p>{{ shown }}</p>
</template>
