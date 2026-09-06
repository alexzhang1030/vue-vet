<script setup>
defineProps(['items', 'extra'])
</script>
<template>
  <li v-for="item in items" v-bind="{ key: item.id, ...extra }">{{ item.name }}</li>
</template>
