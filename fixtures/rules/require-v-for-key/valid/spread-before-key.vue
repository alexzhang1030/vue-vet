<script setup>
defineProps(['items', 'extra'])
</script>
<template>
  <li v-for="item in items" v-bind="{ ...extra, key: item.id }">{{ item.name }}</li>
</template>
