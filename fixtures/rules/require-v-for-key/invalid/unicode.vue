<script setup>
const items = []
const field = 'key'
</script>
<template>
  <p>中文前缀</p>
  <div v-for="item in items" v-bind="{ key: item.id, [field]: undefined }" />
</template>
