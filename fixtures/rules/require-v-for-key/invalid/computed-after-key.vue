<script setup>
const items = []
const field = 'key'
</script>
<template>
  <div v-for="item in items" v-bind="{ key: item.id, [field]: undefined }" />
</template>
