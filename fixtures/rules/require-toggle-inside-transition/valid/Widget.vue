<script setup>
const visible = defineModel()
</script>
<template>
  <span v-if="visible">Item</span>
</template>
