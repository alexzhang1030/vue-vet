<script setup>
const items = []
const field = 'key'
</script>
<template>
  <div v-for="item in items" v-bind="{ [field]: undefined, key: item.id }" />
</template>
